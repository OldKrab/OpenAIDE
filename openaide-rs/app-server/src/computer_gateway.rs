//! The client side of Remote Devices for the Web App (ADR-0082).
//!
//! The Web App's server runs this in a child process, `openaide-app-server
//! gateway --state <folder>`, and talks to it in JSON lines ([`wire`]). The process
//! owns the Web App's device key and its paired Computers in a folder that belongs
//! to the Web App ([`folder`]), spends Pairing Codes, and keeps one loopback
//! gateway per Computer that some browser uses. Each gateway is reached with the
//! endpoint and token shape of a same-machine App Server, so the server's proxy
//! carries both alike. One process holds the one endpoint its key may have.

pub mod folder;
pub mod wire;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use openaide_remote_client::{
    bind, messages, redeem, Endpoint, Failure, Gateway, InviteCode, Logger, PublicKey,
    RemoteClient, SecretKey,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use folder::{Computer, Folder};
use wire::{Call, ComputerInUse, Connection, Event, Outgoing, Refusal, Request};

/// The model the other Computer lists for this device, beside the serving machine's name.
const MODEL: &str = "OpenAIDE Web App";

pub struct ComputerGateway {
    folder: Folder,
    /// Absent when a test supplies the endpoint already bound.
    key: Option<SecretKey>,
    /// Bound when pairing or a Computer is first needed, so a Web App that never
    /// uses another Computer never contacts the public relays (ADR-0062).
    endpoint: tokio::sync::OnceCell<Endpoint>,
    name: String,
    slots: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Slot>>>>,
    events: mpsc::UnboundedSender<Event>,
    log: Logger,
}

/// One per Computer some browser reached; pairing again or forgetting drops it.
#[derive(Default)]
struct Slot {
    running: Option<Running>,
    /// Why the Computer last refused this device for good. A removal stands until
    /// this device pairs again; a version mismatch until the Computer is reached
    /// again. An App Server that removed its last device stops listening, so later
    /// attempts would only find it unreachable.
    refused: Option<Failure>,
}

enum Reached {
    Running(Running),
    /// The Computer answered, but no loopback port could be opened for it.
    NoPort,
}

struct Running {
    gateway: Gateway,
    /// Set by the gateway when the Computer refuses this connection.
    refused: Arc<OnceLock<Failure>>,
}

impl ComputerGateway {
    pub fn new(
        folder: Folder,
        key: SecretKey,
        name: String,
        log: Logger,
    ) -> (Self, mpsc::UnboundedReceiver<Event>) {
        Self::build(folder, Some(key), tokio::sync::OnceCell::new(), name, log)
    }

    #[cfg(test)]
    fn with_endpoint(
        folder: Folder,
        endpoint: Endpoint,
        name: String,
        log: Logger,
    ) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let bound = tokio::sync::OnceCell::new_with(Some(endpoint));
        Self::build(folder, None, bound, name, log)
    }

    fn build(
        folder: Folder,
        key: Option<SecretKey>,
        endpoint: tokio::sync::OnceCell<Endpoint>,
        name: String,
        log: Logger,
    ) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (events, received) = mpsc::unbounded_channel();
        let gateway = Self {
            folder,
            key,
            endpoint,
            name,
            slots: Mutex::new(HashMap::new()),
            events,
            log,
        };
        (gateway, received)
    }

    pub fn computers(&self) -> Result<Vec<Computer>, Refusal> {
        self.folder.computers().map_err(storage_refusal)
    }

    pub async fn pair(
        &self,
        code: &str,
        in_use: Option<ComputerInUse>,
    ) -> Result<Computer, Refusal> {
        let invite = InviteCode::decode(code).map_err(|error| {
            let (reason, message) = messages::refused_code(error);
            Refusal::new(reason, message)
        })?;
        if let Some(in_use) = in_use.filter(|in_use| invite.is_from(&in_use.key)) {
            return Err(Refusal::new("own_code", messages::own_code(&in_use.name)));
        }
        let endpoint = self
            .endpoint()
            .await
            .map_err(|failure| Refusal::new(failure.class(), messages::pairing_failed(failure)))?;
        let paired = redeem(&endpoint, &invite, &self.name, Some(MODEL), &self.log)
            .await
            .map_err(|failure| Refusal::new(failure.class(), messages::pairing_failed(failure)))?;
        let computer = Computer {
            id: paired.id.to_string(),
            name: if paired.name.is_empty() {
                "Computer".to_string()
            } else {
                paired.name
            },
        };
        self.folder
            .remember(computer.clone())
            .map_err(storage_refusal)?;
        // A connection or a refusal kept from before a removal belongs to the old trust.
        self.stop(&computer.id).await;
        Ok(computer)
    }

    /// The local endpoint for `id`, reaching the Computer before it is handed out,
    /// so a browser only moves to a Computer that answers (ADR-0070).
    pub async fn connect(&self, id: &str) -> Result<Connection, Refusal> {
        let computer = self
            .folder
            .find(id)
            .map_err(storage_refusal)?
            .ok_or_else(|| Refusal::new("not_paired", "That computer is no longer paired here."))?;
        let server: PublicKey = id.parse().map_err(|_| {
            Refusal::new(
                "pairing_damaged",
                "This pairing is damaged. Pair with the computer again.",
            )
        })?;
        let slot = self.slot(id);
        let mut slot = slot.lock().await;
        if let Some(current) = slot.running.as_ref() {
            match current.refused.get().copied() {
                None => return Ok(connection_of(&current.gateway)),
                // The refusal is kept; a version mismatch is asked again, so an
                // update on that side is found.
                Some(failure) => {
                    slot.refused = Some(failure);
                    if let Some(previous) = slot.running.take() {
                        previous.gateway.stop().await;
                    }
                }
            }
        }
        let unusable = |failure: Failure| {
            Refusal::new(failure.class(), messages::unusable(failure, &computer.name))
        };
        // A removed device stays removed until it pairs again, which drops this
        // refusal, so asking that Computer again could only wait to be refused.
        if slot.refused == Some(Failure::NotTrusted) {
            return Err(unusable(Failure::NotTrusted));
        }
        let failure = match self.reach(id, server).await {
            Ok(Reached::Running(running)) => {
                let connection = connection_of(&running.gateway);
                slot.running = Some(running);
                slot.refused = None;
                return Ok(connection);
            }
            Ok(Reached::NoPort) => {
                return Err(Refusal::new(
                    "gateway_failed",
                    "OpenAIDE could not open a local port for the other computer. Try again.",
                ));
            }
            Err(failure) => failure,
        };
        // While a refusal stands, not finding the Computer is still that refusal.
        let failure = match (failure, slot.refused) {
            (Failure::Unreachable, Some(earlier)) => earlier,
            _ => failure,
        };
        if failure != Failure::Unreachable {
            slot.refused = Some(failure);
        }
        Err(unusable(failure))
    }

    /// Reaches the Computer `server`, then starts the loopback gateway to it.
    async fn reach(&self, id: &str, server: PublicKey) -> Result<Reached, Failure> {
        let endpoint = self.endpoint().await?;
        let client = Arc::new(RemoteClient::new(endpoint, server, self.log.clone()));
        client.connection().await?;
        let refused = Arc::new(OnceLock::new());
        let on_failure = {
            let refused = refused.clone();
            let events = self.events.clone();
            let id = id.to_string();
            move |failure: Failure| {
                if refused.set(failure).is_ok() {
                    let _ = events.send(Event::Failure {
                        computer: id.clone(),
                        reason: failure.class(),
                    });
                }
            }
        };
        Ok(
            match Gateway::start(client, self.log.clone(), on_failure).await {
                Ok(gateway) => Reached::Running(Running { gateway, refused }),
                Err(_) => Reached::NoPort,
            },
        )
    }

    /// Stops using `id` and removes it. Forgetting a Computer that is not paired succeeds.
    pub async fn forget(&self, id: &str) -> Result<(), Refusal> {
        self.stop(id).await;
        self.folder.forget(id).map_err(storage_refusal)?;
        Ok(())
    }

    /// Stops every gateway, as the App Shell leaves.
    pub async fn stop_all(&self) {
        let slots: Vec<_> = self.lock_slots().drain().map(|(_, slot)| slot).collect();
        for slot in slots {
            if let Some(running) = slot.lock().await.running.take() {
                running.gateway.stop().await;
            }
        }
    }

    /// Stops the gateway to `id` and drops what was learned about it.
    async fn stop(&self, id: &str) {
        let slot = self.lock_slots().remove(id);
        if let Some(slot) = slot {
            if let Some(running) = slot.lock().await.running.take() {
                running.gateway.stop().await;
            }
        }
    }

    async fn endpoint(&self) -> Result<Endpoint, Failure> {
        self.endpoint
            .get_or_try_init(|| async {
                let key = self.key.clone().ok_or(Failure::Unreachable)?;
                bind(key, self.log.clone()).await
            })
            .await
            .cloned()
    }

    /// One slot per Computer, so connecting to one never waits for another.
    fn slot(&self, id: &str) -> Arc<tokio::sync::Mutex<Slot>> {
        self.lock_slots().entry(id.to_string()).or_default().clone()
    }

    fn lock_slots(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, Arc<tokio::sync::Mutex<Slot>>>> {
        self.slots
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    async fn answer(&self, call: Call) -> Result<serde_json::Value, Refusal> {
        let value = match call {
            Call::Computers => wire::computers_result(self.computers()?),
            Call::Pair { code, in_use } => {
                serde_json::json!({ "computer": self.pair(&code, in_use).await? })
            }
            Call::Connect { computer } => serde_json::to_value(self.connect(&computer).await?)
                .expect("a connection serializes"),
            Call::Forget { computer } => {
                self.forget(&computer).await?;
                serde_json::json!({})
            }
        };
        Ok(value)
    }
}

fn connection_of(gateway: &Gateway) -> Connection {
    Connection {
        kind: "localHttp",
        endpoint_url: format!("http://{}/probe", gateway.address),
        auth_token: gateway.token.clone(),
    }
}

fn storage_refusal(_: std::io::Error) -> Refusal {
    Refusal::new(
        "storage",
        "OpenAIDE could not read or save its paired computers. Check the data folder's permissions.",
    )
}

/// Answers requests from `input` on `output` until `input` ends, then stops every gateway.
pub async fn serve(
    gateway: Arc<ComputerGateway>,
    mut events: mpsc::UnboundedReceiver<Event>,
    input: impl AsyncBufRead + Unpin,
    output: impl AsyncWrite + Unpin + Send + 'static,
) {
    let (outgoing, mut sending) = mpsc::unbounded_channel::<Outgoing>();
    let writer = tokio::spawn(async move {
        let mut output = output;
        while let Some(message) = sending.recv().await {
            let mut line = serde_json::to_vec(&message).expect("an outgoing message serializes");
            line.push(b'\n');
            if output.write_all(&line).await.is_err() || output.flush().await.is_err() {
                break;
            }
        }
    });
    let forwarding = {
        let outgoing = outgoing.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if outgoing.send(Outgoing::Event(event)).is_err() {
                    break;
                }
            }
        })
    };
    let _ = outgoing.send(Outgoing::Event(Event::Ready));
    (gateway.log)("computer_gateway_ready", serde_json::json!({}));

    let mut lines = input.lines();
    let mut answering = tokio::task::JoinSet::new();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(_) => {
                (gateway.log)("computer_gateway_input_failed", serde_json::json!({}));
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(_) => {
                (gateway.log)("computer_gateway_request_unreadable", serde_json::json!({}));
                continue;
            }
        };
        let gateway = gateway.clone();
        let outgoing = outgoing.clone();
        answering.spawn(async move {
            let message = answer_logged(&gateway, request).await;
            let _ = outgoing.send(message);
        });
    }
    // The App Shell closed stdin: it left, or is restarting this process.
    answering.shutdown().await;
    gateway.stop_all().await;
    forwarding.abort();
    drop(outgoing);
    let _ = writer.await;
    (gateway.log)("computer_gateway_stopped", serde_json::json!({}));
}

async fn answer_logged(gateway: &ComputerGateway, request: Request) -> Outgoing {
    let started = Instant::now();
    let method = request.call.method();
    (gateway.log)(
        "computer_gateway_request_started",
        serde_json::json!({ "request_id": request.id, "method": method }),
    );
    let answer = gateway.answer(request.call).await;
    let (outcome, reason) = match &answer {
        Ok(_) => ("ok", None),
        Err(refusal) => ("refused", Some(refusal.reason)),
    };
    (gateway.log)(
        "computer_gateway_request_completed",
        serde_json::json!({
            "request_id": request.id,
            "method": method,
            "outcome": outcome,
            "reason": reason,
            "duration_ms": started.elapsed().as_millis() as u64,
        }),
    );
    match answer {
        Ok(result) => Outgoing::Result {
            id: request.id,
            result,
        },
        Err(error) => Outgoing::Error {
            id: request.id,
            error,
        },
    }
}

/// The `gateway` subcommand: serves stdin and stdout for the App Shell that started it.
pub fn run_stdio(state: &Path) -> i32 {
    let log: Logger = Arc::new(crate::logging::info);
    let folder = match Folder::open(state) {
        Ok(folder) => folder,
        Err(_) => {
            crate::logging::error("computer_gateway_folder_failed", serde_json::json!({}));
            eprintln!("OpenAIDE could not open the folder for paired computers.");
            return 1;
        }
    };
    let key = match folder.key() {
        Ok((key, created)) => {
            if created {
                crate::logging::info("computer_gateway_device_key_created", serde_json::json!({}));
            }
            key
        }
        Err(_) => {
            crate::logging::error("computer_gateway_device_key_failed", serde_json::json!({}));
            eprintln!("OpenAIDE could not read this device's key for paired computers.");
            return 1;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("OpenAIDE could not start the gateway.");
            return 1;
        }
    };
    let name = crate::remote_devices::host_name::host_name();
    let (gateway, events) = ComputerGateway::new(folder, key, name, log);
    runtime.block_on(serve(
        Arc::new(gateway),
        events,
        tokio::io::BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
    ));
    0
}
