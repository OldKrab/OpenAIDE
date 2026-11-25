//! The `openaide-relay` command line. Messages are short and plain, for operators who are
//! not network experts. A command that prints a key line prints only the line on standard
//! output, so it can be captured, and its explanation on standard error.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::config::{RelayConfig, DEFAULT_HTTPS_PORT, DEFAULT_STATE_DIR};
use crate::keys::{AddError, KeyStore, RevokeError, StoreError};
use crate::setup::SetupOptions;

#[derive(Debug, Parser)]
#[command(
    name = "openaide-relay",
    version,
    about = "Runs a relay that lets OpenAIDE reach your computers from anywhere.",
    long_about = "Runs a relay that lets OpenAIDE reach your computers from anywhere.\n\n\
        Start with: sudo ./openaide-relay setup\n\
        Each person gets their own relay key, which they paste into OpenAIDE under \
        Settings, Remote, Relay."
)]
pub struct Cli {
    /// Where the relay keeps its settings, keys and certificate.
    #[arg(long, global = true, default_value = DEFAULT_STATE_DIR, hide_default_value = false)]
    pub state_dir: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Install and start the relay, then print the first relay key.
    Setup {
        /// The relay's DNS name, such as relay.example.com.
        #[arg(long)]
        name: Option<String>,
        /// The relay's public IP address. Found automatically when neither this nor
        /// --name is given.
        #[arg(long)]
        ip: Option<IpAddr>,
        /// The HTTPS port. Automatic certificates need 443.
        #[arg(long, default_value_t = DEFAULT_HTTPS_PORT)]
        port: u16,
        /// A folder with a certificate that something else keeps renewed
        /// (fullchain.pem and privkey.pem). Needed when the port is not 443.
        #[arg(long)]
        cert_dir: Option<PathBuf>,
        /// Another ACME directory instead of Let's Encrypt, for testing.
        #[arg(long, hide = true)]
        acme_directory: Option<String>,
    },
    /// Run the relay. The service does this; you do not need to.
    Serve,
    /// Give someone a relay key and print it.
    Add {
        /// Their name, up to 40 characters.
        name: String,
        /// Also let this key list, add and revoke keys.
        #[arg(long)]
        manage: bool,
    },
    /// Print someone's relay key again.
    Show { name: String },
    /// Stop someone's relay key from working, at once.
    Revoke { name: String },
    /// List who has a relay key.
    List,
    /// Check that a certificate folder can be read. Setup runs this as the service user.
    #[command(hide = true)]
    CheckCerts { dir: PathBuf },
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Setup {
            name,
            ip,
            port,
            cert_dir,
            acme_directory,
        } => block_on(crate::setup::run(SetupOptions {
            name,
            ip,
            port,
            cert_dir,
            acme_directory,
            state_dir: cli.state_dir,
        })),
        Command::Serve => block_on(crate::serve::serve(cli.state_dir)),
        Command::Add { name, manage } => add(&cli.state_dir, &name, manage),
        Command::Show { name } => show(&cli.state_dir, &name),
        Command::Revoke { name } => revoke(&cli.state_dir, &name),
        Command::List => list(&cli.state_dir),
        Command::CheckCerts { dir } => crate::tls::load_cert_dir(&dir)
            .map(drop)
            .map_err(|error| error.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn block_on(work: impl std::future::Future<Output = Result<(), String>>) -> Result<(), String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot start: {error}"))?
        .block_on(work)
}

fn config(state_dir: &Path) -> Result<RelayConfig, String> {
    match RelayConfig::load(state_dir) {
        Ok(Some(config)) => Ok(config),
        Ok(None) => Err(not_set_up()),
        Err(error) => Err(store_message(&StoreError::Io(error))),
    }
}

fn not_set_up() -> String {
    "This relay is not set up yet. Run: sudo openaide-relay setup".to_string()
}

fn store_message(error: &StoreError) -> String {
    match error {
        StoreError::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            "Only the relay's administrator can do this. Run the command with sudo.".to_string()
        }
        StoreError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => not_set_up(),
        other => format!("{other}."),
    }
}

fn add(state_dir: &Path, name: &str, manage: bool) -> Result<(), String> {
    let config = config(state_dir)?;
    let person = KeyStore::open(state_dir)
        .add(name, manage)
        .map_err(|error| match error {
            AddError::InvalidName => {
                "A name must be 1 to 40 characters, without control characters.".to_string()
            }
            AddError::NameTaken => {
                format!("Someone named \"{}\" already has a key.", name.trim())
            }
            AddError::Store(error) => store_message(&error),
        })?;
    eprintln!(
        "Give this line to {}. They paste it into OpenAIDE → Settings → Remote → Relay:",
        person.name
    );
    println!("{}", config.key_line(&person.token));
    Ok(())
}

fn show(state_dir: &Path, name: &str) -> Result<(), String> {
    let config = config(state_dir)?;
    let person = KeyStore::open(state_dir)
        .person_named(name)
        .map_err(|error| store_message(&error))?
        .ok_or_else(|| no_such_person(name))?;
    eprintln!("{}'s relay key:", person.name);
    println!("{}", config.key_line(&person.token));
    Ok(())
}

fn revoke(state_dir: &Path, name: &str) -> Result<(), String> {
    config(state_dir)?;
    let person = KeyStore::open(state_dir)
        .revoke(name)
        .map_err(|error| match error {
            RevokeError::NoSuchPerson => no_such_person(name),
            RevokeError::LastManager => format!(
                "\"{}\" has the last key that can manage this relay, so it cannot be revoked. \
                 Add another one first with: sudo openaide-relay add <name> --manage",
                name.trim()
            ),
            RevokeError::Store(error) => store_message(&error),
        })?;
    println!(
        "{}'s relay key no longer works. Their open connections close within a few seconds.",
        person.name
    );
    Ok(())
}

fn list(state_dir: &Path) -> Result<(), String> {
    config(state_dir)?;
    let people = KeyStore::open(state_dir)
        .people()
        .map_err(|error| store_message(&error))?;
    if people.is_empty() {
        println!("No one has a relay key yet.");
        return Ok(());
    }
    print!(
        "{}",
        list_text(
            people
                .iter()
                .map(|person| (person.name.as_str(), person.can_manage))
        )
    );
    Ok(())
}

/// The `list` table: names and whether each key can manage. Never tokens.
pub fn list_text<'a>(people: impl Iterator<Item = (&'a str, bool)> + Clone) -> String {
    let width = people
        .clone()
        .map(|(name, _)| name.chars().count())
        .chain(["NAME".len()])
        .max()
        .unwrap_or(0);
    let mut text = format!("{:<width$}  CAN MANAGE\n", "NAME");
    for (name, can_manage) in people {
        let manage = if can_manage { "yes" } else { "no" };
        text.push_str(&format!("{name:<width$}  {manage}\n"));
    }
    text
}

fn no_such_person(name: &str) -> String {
    format!(
        "No one named \"{}\" has a key. See who does with: sudo openaide-relay list",
        name.trim()
    )
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
