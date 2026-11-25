use clap::Parser;

use super::*;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("openaide-relay").chain(args.iter().copied()))
        .expect("parsed")
}

#[test]
fn setup_defaults_to_port_443_and_automatic_certificates() {
    let cli = parse(&["setup"]);
    assert_eq!(cli.state_dir, Path::new(DEFAULT_STATE_DIR));
    assert_eq!(
        cli.command,
        Command::Setup {
            name: None,
            ip: None,
            port: 443,
            cert_dir: None,
            acme_directory: None,
        }
    );
}

#[test]
fn setup_takes_an_address_a_port_and_a_certificate_folder() {
    let cli = parse(&[
        "setup",
        "--ip",
        "2a01:4f8::1",
        "--port",
        "8443",
        "--cert-dir",
        "/etc/certs",
        "--state-dir",
        "/srv/relay",
    ]);
    assert_eq!(cli.state_dir, Path::new("/srv/relay"));
    assert_eq!(
        cli.command,
        Command::Setup {
            name: None,
            ip: Some("2a01:4f8::1".parse().expect("ip")),
            port: 8443,
            cert_dir: Some("/etc/certs".into()),
            acme_directory: None,
        }
    );
    let named = parse(&["setup", "--name", "relay.example.com"]);
    assert!(
        matches!(named.command, Command::Setup { name: Some(name), .. } if name == "relay.example.com")
    );
    assert!(Cli::try_parse_from(["openaide-relay", "setup", "--ip", "not-an-ip"]).is_err());
}

#[test]
fn key_commands_take_a_name() {
    assert_eq!(
        parse(&["add", "Olga P", "--manage"]).command,
        Command::Add {
            name: "Olga P".into(),
            manage: true
        }
    );
    assert_eq!(
        parse(&["add", "Pat"]).command,
        Command::Add {
            name: "Pat".into(),
            manage: false
        }
    );
    assert_eq!(
        parse(&["show", "Pat"]).command,
        Command::Show { name: "Pat".into() }
    );
    assert_eq!(
        parse(&["revoke", "Pat"]).command,
        Command::Revoke { name: "Pat".into() }
    );
    assert_eq!(parse(&["list"]).command, Command::List);
    assert_eq!(parse(&["serve"]).command, Command::Serve);
    assert!(Cli::try_parse_from(["openaide-relay", "add"]).is_err());
}

#[test]
fn help_names_every_command_but_the_internal_ones() {
    let help = Cli::try_parse_from(["openaide-relay", "--help"]).expect_err("help");
    let text = help.to_string();
    for command in ["setup", "serve", "add", "show", "revoke", "list"] {
        assert!(text.contains(command), "{command} missing from\n{text}");
    }
    assert!(!text.contains("check-certs"));
}

#[test]
fn the_list_shows_names_and_manage_rights_only() {
    let text = list_text([("owner", true), ("Olga Pérez", false)].into_iter());
    assert_eq!(
        text,
        "NAME        CAN MANAGE\nowner       yes\nOlga Pérez  no\n"
    );
}
