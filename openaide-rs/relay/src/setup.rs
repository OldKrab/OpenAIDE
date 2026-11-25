//! `sudo ./openaide-relay setup`: installs the relay as a systemd service, waits until it
//! answers over HTTPS with a valid certificate, and prints the first relay key.
//!
//! Running it again is safe. It rewrites the program, the settings and the service, and
//! restarts the relay, but it keeps every key and does not print a new owner key.
//!
//! The decisions (which address, which certificate source, the unit text) are pure
//! functions with tests. The steps that need root and systemd are not tested here.

use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::config::{
    CertSource, Host, RelayConfig, DEFAULT_HTTPS_PORT, DEFAULT_QUIC_PORT, DEFAULT_STATE_DIR,
};
use crate::keys::KeyStore;
use crate::probe;

pub const SERVICE_USER: &str = "openaide-relay";
pub const INSTALLED_BINARY: &str = "/usr/local/bin/openaide-relay";
pub const UNIT_PATH: &str = "/etc/systemd/system/openaide-relay.service";
pub const UNIT_NAME: &str = "openaide-relay.service";
pub const OWNER_NAME: &str = "owner";
/// How long setup waits for the relay to answer with a valid certificate.
const READY_WAIT: Duration = Duration::from_secs(120);
const READY_POLL: Duration = Duration::from_secs(3);
const PROGRESS_EVERY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupOptions {
    pub name: Option<String>,
    pub ip: Option<IpAddr>,
    pub port: u16,
    pub cert_dir: Option<PathBuf>,
    pub acme_directory: Option<String>,
    pub state_dir: PathBuf,
}

/// The public address found among this machine's own addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpChoice {
    Public(IpAddr),
    /// Only private addresses: the machine is behind a router.
    OnlyPrivate,
    None,
}

/// Prefers a global IPv4 address, then a global IPv6 one, in interface order.
// TODO: skip temporary (privacy) IPv6 addresses, which change daily. Telling them apart
// needs the address flags from netlink, which `getifaddrs` does not report; a server
// rarely has them, and `--ip` overrides the choice.
pub fn choose_public_ip(addresses: &[IpAddr]) -> IpChoice {
    let v4 = addresses.iter().find(|ip| match ip {
        IpAddr::V4(ip) => is_global_v4(*ip),
        IpAddr::V6(_) => false,
    });
    let v6 = addresses.iter().find(|ip| match ip {
        IpAddr::V6(ip) => is_global_v6(*ip),
        IpAddr::V4(_) => false,
    });
    match v4.or(v6) {
        Some(ip) => IpChoice::Public(*ip),
        None if addresses.iter().any(|ip| !ip.is_loopback()) => IpChoice::OnlyPrivate,
        None => IpChoice::None,
    }
}

fn is_global_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // shared address space (carrier NAT)
        || (a == 198 && (18..20).contains(&b)) // benchmarking
        || (a == 192 && b == 0 && c == 0) // protocol assignments
        || a >= 240) // reserved
}

fn is_global_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    // Global unicast is 2000::/3; 2001:db8::/32 is for documentation.
    (segments[0] & 0xe000) == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}

/// Why setup stops before changing anything, worded for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

/// Decides the relay's settings from the options and this machine's addresses.
pub fn plan(options: &SetupOptions, addresses: &[IpAddr]) -> Result<RelayConfig, Refusal> {
    let host = match (&options.name, options.ip) {
        (Some(_), Some(_)) => {
            return Err(Refusal("Give either --name or --ip, not both.".to_string()))
        }
        (Some(name), None) => Host::parse_name(name).map_err(|_| {
            Refusal(format!(
                "\"{name}\" is not a DNS name. Give a name such as relay.example.com."
            ))
        })?,
        (None, Some(ip)) => Host::Ip(ip),
        (None, None) => match choose_public_ip(addresses) {
            IpChoice::Public(ip) => Host::Ip(ip),
            IpChoice::OnlyPrivate => {
                return Err(Refusal(
                    "This computer has only private addresses, so it is probably behind a \
                     router. Forward TCP port 443 and UDP port 7842 on the router to this \
                     computer, then run setup again with --ip <your public address> or \
                     --name <a DNS name that points to it>."
                        .to_string(),
                ))
            }
            IpChoice::None => {
                return Err(Refusal(
                    "Cannot find this computer's address. Run setup again with \
                     --ip <public address> or --name <DNS name>."
                        .to_string(),
                ))
            }
        },
    };
    let cert = match &options.cert_dir {
        Some(dir) => CertSource::Files { dir: dir.clone() },
        None if options.port != DEFAULT_HTTPS_PORT => {
            return Err(Refusal(format!(
                "The relay gets its certificate by itself only on port 443. To use port {}, \
                 get a certificate another way (for example with certbot) and add \
                 --cert-dir <folder with fullchain.pem and privkey.pem>.",
                options.port
            )))
        }
        None => CertSource::Acme,
    };
    Ok(RelayConfig {
        host,
        port: options.port,
        cert,
        quic_port: Some(DEFAULT_QUIC_PORT),
        acme_directory: options.acme_directory.clone(),
    })
}

/// The systemd unit. The relay runs as its own user, may bind ports below 1024, and may
/// write only its state directory.
pub fn unit_text(state_dir: &Path) -> String {
    let mut exec = format!("{INSTALLED_BINARY} serve");
    if state_dir != Path::new(DEFAULT_STATE_DIR) {
        exec.push_str(&format!(" --state-dir {}", state_dir.display()));
    }
    format!(
        "[Unit]
Description=OpenAIDE relay
Wants=network-online.target
After=network-online.target

[Service]
ExecStart={exec}
User={SERVICE_USER}
Group={SERVICE_USER}
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths={state}
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=true
LockPersonality=true
MemoryDenyWriteExecute=true
SystemCallArchitectures=native
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
",
        state = state_dir.display(),
    )
}

/// Runs every step. Messages go to standard output as they happen.
pub async fn run(options: SetupOptions) -> Result<(), String> {
    if !nix::unistd::geteuid().is_root() {
        return Err(
            "Setup changes system settings. Run it with sudo: sudo ./openaide-relay setup".into(),
        );
    }
    if !Path::new("/run/systemd/system").exists() {
        return Err(
            "This computer does not use systemd, which setup needs to keep the relay running."
                .into(),
        );
    }
    let addresses = if options.name.is_none() && options.ip.is_none() {
        local_addresses()
            .map_err(|error| format!("Cannot list this computer's addresses: {error}"))?
    } else {
        Vec::new()
    };
    let config = plan(&options, &addresses).map_err(|refusal| refusal.0)?;
    println!("Setting up a relay at {}.", config.host.url_host());

    if let CertSource::Files { dir } = &config.cert {
        crate::tls::load_cert_dir(dir).map_err(|error| {
            format!("Cannot use the certificate in {}: {error}.", dir.display())
        })?;
    }
    install_binary()
        .map_err(|error| format!("Cannot copy the program to {INSTALLED_BINARY}: {error}"))?;
    let (uid, gid) = ensure_service_user()?;
    prepare_state_dir(&options.state_dir, uid, gid)
        .map_err(|error| format!("Cannot prepare {}: {error}", options.state_dir.display()))?;
    if let CertSource::Files { dir } = &config.cert {
        check_service_user_reads(dir)?;
    }

    let previous = RelayConfig::load(&options.state_dir).ok().flatten();
    config
        .save(&options.state_dir)
        .map_err(|error| format!("Cannot save the relay's settings: {error}"))?;
    install_unit(&options.state_dir)?;
    // TODO: open the ports in ufw or firewalld when one is active. Until then the timeout
    // message below names the ports to open.
    println!("The relay service is started. Waiting for it to answer over HTTPS…");
    let verify = config.acme_directory.is_none();
    wait_until_ready(&config, verify).await?;

    let store = KeyStore::open(&options.state_dir);
    let people = store
        .people()
        .map_err(|error| format!("Cannot read the relay keys: {error}"))?;
    if people.is_empty() {
        let owner = store
            .add(OWNER_NAME, true)
            .map_err(|_| "Cannot create the first relay key.".to_string())?;
        println!();
        println!("The relay is ready. This is your relay key; paste it into OpenAIDE → Settings → Remote → Relay:");
        println!();
        println!("{}", config.key_line(&owner.token));
        println!();
        println!("Keep it private: it lets anyone who has it use this relay and manage its keys.");
    } else {
        println!("The relay is ready. Your existing relay keys still work.");
        if previous
            .is_some_and(|previous| previous.host != config.host || previous.port != config.port)
        {
            println!("The relay's address changed, so every key line changed too. Print one again with: sudo openaide-relay show <name>");
        }
    }
    Ok(())
}

/// This machine's addresses, from its network interfaces. No outside service is asked.
fn local_addresses() -> nix::Result<Vec<IpAddr>> {
    Ok(nix::ifaddrs::getifaddrs()?
        .filter_map(|interface| {
            let address = interface.address?;
            if let Some(v4) = address.as_sockaddr_in() {
                return Some(IpAddr::V4(v4.ip()));
            }
            address.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip()))
        })
        .collect())
}

/// Copies this program to [`INSTALLED_BINARY`] through a temporary file, so a running
/// copy is replaced rather than overwritten.
fn install_binary() -> io::Result<()> {
    let current = std::env::current_exe()?;
    let target = Path::new(INSTALLED_BINARY);
    if fs::canonicalize(&current).ok() == fs::canonicalize(target).ok() {
        return Ok(());
    }
    let temporary = target.with_extension("new");
    fs::copy(&current, &temporary)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
    fs::rename(&temporary, target)
}

fn ensure_service_user() -> Result<(u32, u32), String> {
    let find = || nix::unistd::User::from_name(SERVICE_USER).ok().flatten();
    if find().is_none() {
        let shell = ["/usr/sbin/nologin", "/sbin/nologin"]
            .into_iter()
            .find(|shell| Path::new(shell).exists())
            .unwrap_or("/bin/false");
        let status = Command::new("useradd")
            .args(["--system", "--user-group", "--no-create-home", "--home-dir"])
            .arg(DEFAULT_STATE_DIR)
            .args(["--shell", shell, SERVICE_USER])
            .status()
            .map_err(|error| format!("Cannot run useradd: {error}"))?;
        if !status.success() {
            return Err(format!("Cannot create the system user {SERVICE_USER}."));
        }
    }
    let user = find().ok_or_else(|| format!("Cannot find the system user {SERVICE_USER}."))?;
    Ok((user.uid.as_raw(), user.gid.as_raw()))
}

fn prepare_state_dir(dir: &Path, uid: u32, gid: u32) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    std::os::unix::fs::chown(dir, Some(uid), Some(gid))?;
    // Files an earlier run or an operator's command left behind go to the service too.
    for entry in fs::read_dir(dir)? {
        std::os::unix::fs::lchown(entry?.path(), Some(uid), Some(gid))?;
    }
    Ok(())
}

/// Runs this program as the service user to load the certificate, which is what `serve`
/// will do. certbot keeps its files readable by root only, which is the usual failure.
/// `runuser` gives the child the service user's groups, not root's.
fn check_service_user_reads(dir: &Path) -> Result<(), String> {
    let status = Command::new("runuser")
        .args(["-u", SERVICE_USER, "--", INSTALLED_BINARY, "check-certs"])
        .arg(dir)
        .status()
        .map_err(|error| format!("Cannot check the certificate files: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!(
        "The relay runs as the user {SERVICE_USER}, which cannot read the certificate in {}. \
         Give that user read access to the folder and its files (certbot's files are \
         readable by root only), then run setup again.",
        dir.display()
    ))
}

fn install_unit(state_dir: &Path) -> Result<(), String> {
    fs::write(UNIT_PATH, unit_text(state_dir))
        .map_err(|error| format!("Cannot write {UNIT_PATH}: {error}"))?;
    for args in [
        &["daemon-reload"][..],
        &["enable", UNIT_NAME][..],
        &["restart", UNIT_NAME][..],
    ] {
        let status = Command::new("systemctl")
            .args(args)
            .status()
            .map_err(|error| format!("Cannot run systemctl: {error}"))?;
        if !status.success() {
            return Err(format!(
                "systemctl {} failed. See what happened with: journalctl -u {UNIT_NAME}",
                args.join(" ")
            ));
        }
    }
    Ok(())
}

async fn wait_until_ready(config: &RelayConfig, verify: bool) -> Result<(), String> {
    let started = Instant::now();
    let mut last_progress = Instant::now();
    loop {
        let failure = match probe::ping(&config.host, config.port, verify).await {
            Ok(()) => return Ok(()),
            Err(failure) => failure,
        };
        if started.elapsed() >= READY_WAIT {
            let mut message = format!(
                "The relay did not get ready within {} seconds: {}.\n",
                READY_WAIT.as_secs(),
                failure.describe()
            );
            if matches!(config.cert, CertSource::Acme) {
                message.push_str(&format!(
                    "Let's Encrypt must reach this computer at {} on TCP port 443. ",
                    config.host.url_host()
                ));
            }
            message.push_str(&format!(
                "Check that the firewall allows TCP port {} and UDP port {DEFAULT_QUIC_PORT}, \
                 see what happened with: journalctl -u {UNIT_NAME}\n\
                 Then run setup again; it keeps what is already done.",
                config.port
            ));
            return Err(message);
        }
        if last_progress.elapsed() >= PROGRESS_EVERY {
            last_progress = Instant::now();
            println!("Still waiting: {}…", failure.describe());
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
