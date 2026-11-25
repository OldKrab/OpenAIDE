//! The Mobile App as a Remote Device (ADR-0062): this phone's node of
//! `openaide-remote-client`, exposed to Kotlin through UniFFI.
//!
//! The node owns the endpoint, pairing, the connection to the computer in use and
//! the rules that keep it on a phone. The Kotlin shell keeps the key and the
//! computers it was paired with (encrypted with Android Keystore), reports the
//! app leaving and returning to the screen, and words every outcome for the user.
//! Logs carry only stable names, classes, counts, and durations.

mod host;
mod stream;

use std::future::Future;
use std::sync::{Arc, OnceLock};

use openaide_remote_client::{
    self as remote, CheckComputer as Computer, CheckRelay as Relay, Failure, InviteCode,
    PairingCodeError, PublicKey, RemoteNode,
};
use tokio::runtime::Runtime;

pub use host::{NetworkKind, RemoteHost, StorageError, Transport};
pub use stream::{Reached, Stream, StreamError};

uniffi::setup_scaffolding!();

/// Why the paired computer cannot be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum RemoteError {
    #[error("unreachable")]
    Unreachable,
    #[error("not_trusted")]
    NotTrusted,
    #[error("incompatible")]
    Incompatible,
    #[error("storage")]
    Storage,
}

impl From<Failure> for RemoteError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Unreachable => Self::Unreachable,
            Failure::NotTrusted => Self::NotTrusted,
            Failure::Incompatible => Self::Incompatible,
            Failure::Storage => Self::Storage,
        }
    }
}

/// Why a text will not be spent as a Pairing Code (ADR-0081).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CodeError {
    #[error("not_a_pairing_code")]
    NotAPairingCode,
    /// A join-request code of an OpenAIDE before ADR-0081.
    #[error("from_older_openaide")]
    FromOlderOpenAide,
    #[error("malformed")]
    Malformed,
}

/// A Pairing Code that names an App Server and holds a single-use secret.
#[derive(Debug, uniffi::Object)]
pub struct Invite(InviteCode);

#[uniffi::export]
impl Invite {
    /// The App Server's public key as the shell stores a paired computer's id.
    pub fn server_id(&self) -> String {
        self.0.server.to_string()
    }

    /// Whether the App Server with the paired id `server_id` made this code.
    pub fn is_from(&self, server_id: String) -> bool {
        self.0.is_from(&server_id)
    }
}

/// Reads a scanned or pasted code; spaces, dashes and case a person added are ignored.
/// A code may name an Own Relay (ADR-0085): pairing keeps it with the computer
/// through [`RemoteHost::save_own_relay`], and every dial of that computer uses it.
#[uniffi::export]
pub fn parse_invite(code: String) -> Result<Arc<Invite>, CodeError> {
    InviteCode::decode(&code)
        .map(|invite| Arc::new(Invite(invite)))
        .map_err(|error| match error {
            PairingCodeError::NotAPairingCode => CodeError::NotAPairingCode,
            PairingCodeError::FromOlderOpenAide => CodeError::FromOlderOpenAide,
            PairingCodeError::Malformed => CodeError::Malformed,
        })
}

/// Trims a self-reported label to what one row can show, as the App Server will.
#[uniffi::export]
pub fn clamp_label(label: String) -> String {
    remote::clamp_label(&label)
}

/// An App Server that trusts this phone, as the user knows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairedComputer {
    pub id: String,
    pub name: String,
}

/// Whether this phone's endpoint got a home relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CheckRelay {
    Reached,
    NotReached,
    NoEndpoint,
}

/// How the computer answered a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CheckComputer {
    Connected,
    Relay,
    Direct,
    Unreachable,
    NotTrusted,
    Incompatible,
    NotChecked,
}

/// What each step of "Check the connection" found, and the first step that
/// failed as `outcome` (`ok` when none did).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Check {
    pub network: NetworkKind,
    pub relay: CheckRelay,
    pub computer: CheckComputer,
    pub outcome: String,
}

/// This phone's node; the shell keeps one for the life of the process.
#[derive(uniffi::Object)]
pub struct Remote {
    host: Arc<dyn RemoteHost>,
    node: RemoteNode,
}

#[uniffi::export]
impl Remote {
    #[uniffi::constructor]
    pub fn new(host: Arc<dyn RemoteHost>) -> Arc<Self> {
        let _runtime = runtime().enter();
        let log = host::logger(host.clone());
        let node = RemoteNode::new(Arc::new(host::Bridge(host.clone())), log);
        Arc::new(Self { host, node })
    }

    /// The app left the screen; Android may stop its network from here on.
    pub fn hidden(&self) {
        self.node.hidden();
    }

    /// The app is on screen again; after a longer absence the endpoint is replaced.
    pub fn shown(&self) {
        let _runtime = runtime().enter();
        self.node.shown();
    }

    /// Spends `invite`: connects to the App Server it names and presents its secret.
    /// Before it returns, the node has kept the computer's Own Relay line through the
    /// host, or cleared one an earlier pairing left, so the line never crosses into
    /// the shell's records.
    pub async fn redeem(
        self: Arc<Self>,
        invite: Arc<Invite>,
        name: String,
        model: Option<String>,
    ) -> Result<PairedComputer, RemoteError> {
        on_runtime(async move {
            let paired = self.node.redeem(&invite.0, &name, model.as_deref()).await?;
            Ok(PairedComputer {
                id: paired.id.to_string(),
                name: paired.name,
            })
        })
        .await
    }

    /// Reaches the computer in use, unless the connection to it is live.
    pub async fn connect(self: Arc<Self>) -> Result<(), RemoteError> {
        on_runtime(async move { Ok(self.node.connection().await.map(|_| ())?) }).await
    }

    /// Opens the stream for one request to the computer in use.
    pub async fn open(self: Arc<Self>) -> Result<Arc<Stream>, RemoteError> {
        on_runtime(async move {
            let (send, recv) = self.node.open().await?;
            Ok(Arc::new(Stream::new(send, recv)))
        })
        .await
    }

    /// Reaches `computer` for a switch while the computer in use stays in use.
    pub async fn reach_for_switch(
        self: Arc<Self>,
        computer: String,
    ) -> Result<Arc<Reached>, RemoteError> {
        on_runtime(async move {
            let server = parse_computer(&computer)?;
            let connection = self.node.reach_for_switch(server).await?;
            Ok(Arc::new(Reached(connection)))
        })
        .await
    }

    /// Makes what a switch reached the connection in use, ending the previous one.
    pub fn adopt(&self, reached: Arc<Reached>) {
        let _runtime = runtime().enter();
        self.node.adopt(reached.0.clone());
    }

    /// Ends what a switch reached when the switch no longer applies.
    pub fn abandon(&self, reached: Arc<Reached>) {
        self.node.abandon(&reached.0);
    }

    /// Checks the way to `computer` step by step.
    pub async fn check(self: Arc<Self>, computer: String) -> Check {
        on_runtime(async move {
            let Ok(server) = parse_computer(&computer) else {
                return Check {
                    network: self.host.network(),
                    relay: CheckRelay::NoEndpoint,
                    computer: CheckComputer::NotChecked,
                    outcome: "not_checked".to_string(),
                };
            };
            let result = self.node.check(server).await;
            Check {
                network: result.network.into(),
                relay: match result.relay {
                    Relay::Reached => CheckRelay::Reached,
                    Relay::NotReached => CheckRelay::NotReached,
                    Relay::NoEndpoint => CheckRelay::NoEndpoint,
                },
                computer: match result.computer {
                    Computer::Connected => CheckComputer::Connected,
                    Computer::Relay => CheckComputer::Relay,
                    Computer::Direct => CheckComputer::Direct,
                    Computer::Unreachable => CheckComputer::Unreachable,
                    Computer::NotTrusted => CheckComputer::NotTrusted,
                    Computer::Incompatible => CheckComputer::Incompatible,
                    Computer::NotChecked => CheckComputer::NotChecked,
                },
                outcome: result.outcome().to_string(),
            }
        })
        .await
    }

    /// Ends the connection after the user forgets the computer or uses another.
    pub fn disconnect(&self) {
        self.node.disconnect();
    }
}

/// The network kind from Android's transport flags; a VPN is reported beside
/// the transport under it.
#[uniffi::export]
pub fn network_kind(
    wifi: bool,
    cellular: bool,
    ethernet: bool,
    vpn: bool,
    validated: bool,
) -> NetworkKind {
    remote::NetworkKind::of(wifi, cellular, ethernet, vpn, validated).into()
}

fn parse_computer(id: &str) -> Result<PublicKey, RemoteError> {
    id.parse().map_err(|_| RemoteError::NotTrusted)
}

/// The library's own runtime: the node and its endpoint run here, whichever
/// thread the shell calls from.
fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("openaide-remote")
            .enable_all()
            .build()
            .expect("the remote runtime starts")
    })
}

/// Runs `future` on the library's runtime for a Kotlin coroutine. When the
/// coroutine is cancelled, the call's future is dropped and the work stops with it.
async fn on_runtime<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> T {
    struct AbortOnDrop(tokio::task::AbortHandle);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let task = runtime().spawn(future);
    let _abort = AbortOnDrop(task.abort_handle());
    match task.await {
        Ok(value) => value,
        // Only a panic ends the task early: it is aborted only once nothing awaits it.
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Gives the DNS resolver this app's Java VM and application context, so it reads
/// Android's nameservers instead of falling back to public ones. Kotlin calls it
/// once before the node is made; later calls do nothing.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_io_openaide_android_RemoteLibrary_installContext(
    env: jni::JNIEnv,
    _class: jni::objects::JClass,
    context: jni::objects::JObject,
) {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let (Ok(vm), Ok(context)) = (env.get_java_vm(), env.new_global_ref(context)) else {
            return;
        };
        let context_pointer = context.as_obj().as_raw();
        // Both must stay valid until the process exits.
        std::mem::forget(context);
        // SAFETY: the VM outlives the process's native code, and the global
        // reference is never deleted.
        unsafe {
            iroh::dns::install_android_jni_context(
                vm.get_java_vm_pointer().cast(),
                context_pointer.cast(),
            );
        }
    });
}

#[cfg(test)]
mod tests;
