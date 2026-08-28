use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use url::Url;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Waits for the authenticated App Server listener to close. Its process only exits
/// after the store has drained and recorded a clean shutdown, so installer handoff
/// cannot race the durable-state writer.
pub(crate) async fn wait_for_app_server_shutdown(endpoint_url: String) -> Result<(), ()> {
    let address = loopback_address(&endpoint_url).ok_or(())?;
    tauri::async_runtime::spawn_blocking(move || {
        let started_at = Instant::now();
        while started_at.elapsed() < SHUTDOWN_TIMEOUT {
            if TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).is_err() {
                return Ok(());
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        Err(())
    })
    .await
    .map_err(|_| ())?
}

fn loopback_address(endpoint_url: &str) -> Option<SocketAddr> {
    let endpoint = Url::parse(endpoint_url).ok()?;
    let host = endpoint.host_str()?.parse::<IpAddr>().ok()?;
    if !host.is_loopback() {
        return None;
    }
    Some(SocketAddr::new(host, endpoint.port_or_known_default()?))
}

#[cfg(test)]
#[path = "desktop_update_shutdown_tests.rs"]
mod tests;
