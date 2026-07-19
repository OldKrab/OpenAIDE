use std::pin::pin;
use std::sync::Arc;

use agent_client_protocol::{AcpAgent, Agent, Client, ConnectTo, LineDirection, Lines};
use async_process::Child;
use futures::io::BufReader;
use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};

type DebugCallback = Arc<dyn Fn(&str, LineDirection) + Send + Sync + 'static>;

/// ACP transport that supervises the Agent process and, on Linux, its complete child group.
pub(crate) struct ManagedAcpAgent {
    agent: AcpAgent,
    debug: Option<DebugCallback>,
}

impl ManagedAcpAgent {
    pub(super) fn new(agent: AcpAgent) -> Self {
        Self { agent, debug: None }
    }

    pub(super) fn with_debug(
        mut self,
        debug: impl Fn(&str, LineDirection) + Send + Sync + 'static,
    ) -> Self {
        self.debug = Some(Arc::new(debug));
        self
    }
}

impl ConnectTo<Client> for ManagedAcpAgent {
    async fn connect_to(self, client: impl ConnectTo<Agent>) -> agent_client_protocol::Result<()> {
        let (child_stdin, child_stdout, child_stderr, child) = self.agent.spawn_process()?;
        let child = ManagedChild::new(child);
        let (stderr_tx, stderr_rx) = futures::channel::oneshot::channel::<String>();
        let stderr_debug = self.debug.clone();
        let stderr_future = async move {
            let mut lines = BufReader::new(child_stderr).lines();
            let mut collected = String::new();
            while let Some(Ok(line)) = lines.next().await {
                if let Some(debug) = &stderr_debug {
                    debug(&line, LineDirection::Stderr);
                }
                if !collected.is_empty() {
                    collected.push('\n');
                }
                collected.push_str(&line);
            }
            let _ = stderr_tx.send(collected);
        };
        let child_monitor = monitor_child(child, stderr_rx);
        let incoming: std::pin::Pin<
            Box<dyn futures::Stream<Item = std::io::Result<String>> + Send>,
        > = match self.debug.clone() {
            Some(debug) => Box::pin(BufReader::new(child_stdout).lines().inspect(move |line| {
                if let Ok(line) = line {
                    debug(line, LineDirection::Stdout);
                }
            })),
            None => Box::pin(BufReader::new(child_stdout).lines()),
        };
        let outgoing: std::pin::Pin<Box<dyn futures::Sink<String, Error = std::io::Error> + Send>> =
            match self.debug {
                Some(debug) => Box::pin(futures::sink::unfold(
                    (child_stdin, debug),
                    async move |(mut writer, debug), line: String| {
                        debug(&line, LineDirection::Stdin);
                        writer.write_all(format!("{line}\n").as_bytes()).await?;
                        Ok::<_, std::io::Error>((writer, debug))
                    },
                )),
                None => Box::pin(futures::sink::unfold(
                    child_stdin,
                    async move |mut writer, line: String| {
                        writer.write_all(format!("{line}\n").as_bytes()).await?;
                        Ok::<_, std::io::Error>(writer)
                    },
                )),
            };
        let protocol = ConnectTo::<Client>::connect_to(Lines::new(outgoing, incoming), client);
        let protocol = pin!(protocol);
        let child_monitor = pin!(child_monitor);
        let stderr_future = pin!(stderr_future);
        let main = async {
            match futures::future::select(protocol, child_monitor).await {
                futures::future::Either::Left((result, _))
                | futures::future::Either::Right((result, _)) => result,
            }
        };
        let main = pin!(main);
        match futures::future::select(main, stderr_future).await {
            futures::future::Either::Left((result, _)) => result,
            futures::future::Either::Right(((), protocol)) => protocol.await,
        }
    }
}

struct ManagedChild {
    child: Child,
    process_id: u32,
}

impl ManagedChild {
    fn new(child: Child) -> Self {
        let process_id = child.id();
        Self { child, process_id }
    }

    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.status().await
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        // Descendants can remain alive after the Agent leader exits, so cleanup
        // targets the group even when the direct child has already been reaped.
        terminate_process_group(self.process_id);
        drop(self.child.kill());
    }
}

#[cfg(target_os = "linux")]
fn terminate_process_group(process_id: u32) {
    // The Agent is launched through `setsid`, so its PID is also its process-group ID.
    let result = unsafe { libc::kill(-(process_id as i32), libc::SIGKILL) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            crate::logging::warn(
                "acp_process_group_termination_failed",
                serde_json::json!({
                    "process_id": process_id,
                    "error_kind": format!("{:?}", error.kind()),
                    "os_error_code": error.raw_os_error(),
                }),
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn terminate_process_group(_process_id: u32) {}

async fn monitor_child(
    mut child: ManagedChild,
    stderr_rx: futures::channel::oneshot::Receiver<String>,
) -> agent_client_protocol::Result<()> {
    let status = child
        .wait()
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if status.success() {
        return Ok(());
    }
    let stderr = stderr_rx.await.unwrap_or_default();
    Err(
        agent_client_protocol::Error::internal_error().data(if stderr.is_empty() {
            format!("Agent process exited with {status}")
        } else {
            format!("Agent process exited with {status}: {stderr}")
        }),
    )
}
