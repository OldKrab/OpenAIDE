use super::AcpAgentConfig;
use super::{
    command_not_found_error, process_args, resolve_command_in_paths, windows_command_extensions,
};
use crate::protocol::errors::RuntimeError;
use std::ffi::OsStr;
use std::fs;

#[test]
fn built_in_codex_uses_the_product_pinned_adapter() {
    let config = AcpAgentConfig::codex_managed_package();

    assert_eq!(config.agent_id, "codex");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args.len(), 2);
    assert_eq!(config.args[0], "-y");
    let version = config.args[1].strip_prefix("@openaide/codex-acp@").unwrap();
    assert_eq!(version.split('.').count(), 3);
    assert!(version
        .split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())));
    assert!(config.uses_product_pinned_codex_package());
    assert!(config.env.is_empty());
    assert_eq!(config.diagnostic_launcher_kind(), "managed_package");
}

#[test]
fn built_in_claude_uses_an_exact_stable_package_pin() {
    let config = AcpAgentConfig::claude_code();
    assert_eq!(config.agent_id, "claude-code");
    assert_eq!(config.args.len(), 2);
    assert_eq!(config.args[0], "-y");
    let version = config.args[1]
        .strip_prefix("@openaide/claude-agent-acp@")
        .expect("the built-in must use the OpenAIDE-maintained Claude ACP package");
    let components: Vec<_> = version.split('.').collect();
    assert_eq!(
        components.len(),
        3,
        "the launch policy must pin an exact version"
    );
    for component in components {
        assert!(!component.is_empty());
        assert!(component.bytes().all(|byte| byte.is_ascii_digit()));
        assert!(component == "0" || !component.starts_with('0'));
    }
    assert!(config.env.is_empty());
    assert!(config.secret_env.is_empty());
}

#[test]
fn missing_codex_npx_is_classified_as_node_js_required() {
    assert!(matches!(
        command_not_found_error("codex", "npx"),
        RuntimeError::NodeJsRequired(_)
    ));
}

#[test]
fn missing_claude_code_npx_is_classified_as_node_js_required() {
    assert!(matches!(
        command_not_found_error("claude-code", "npx"),
        RuntimeError::NodeJsRequired(_)
    ));
}

#[test]
fn missing_custom_npx_remains_a_generic_setup_failure() {
    assert!(matches!(
        command_not_found_error("custom.local", "npx"),
        RuntimeError::SetupRequired(_)
    ));
}

#[test]
fn windows_command_lookup_selects_the_cmd_launcher_instead_of_the_posix_shim() {
    let temp = tempfile::tempdir().expect("temporary command directory");
    fs::write(temp.path().join("npx"), "#!/bin/sh\n").expect("write POSIX npm shim");
    fs::write(temp.path().join("npx.cmd"), "@echo off\r\n").expect("write Windows npm shim");

    let resolved = resolve_command_in_paths(
        "npx",
        [temp.path()],
        [OsStr::new(".exe"), OsStr::new(".cmd")],
    );

    assert_eq!(resolved, Some(temp.path().join("npx.cmd")));
}

#[test]
fn command_lookup_does_not_append_an_extension_to_an_explicit_executable_name() {
    let temp = tempfile::tempdir().expect("temporary command directory");
    fs::write(temp.path().join("agent.exe"), "executable").expect("write executable fixture");

    let resolved = resolve_command_in_paths(
        "agent.exe",
        [temp.path()],
        [OsStr::new(".exe"), OsStr::new(".cmd")],
    );

    assert_eq!(resolved, Some(temp.path().join("agent.exe")));
}

#[test]
fn windows_command_extensions_follow_pathext_and_ignore_unsupported_scripts() {
    assert_eq!(
        windows_command_extensions(Some(OsStr::new(".COM;.EXE;.PS1;.BAT;.CMD"))),
        vec![".COM", ".EXE", ".BAT", ".CMD"]
    );
}

#[test]
fn windows_batch_launcher_is_invoked_through_cmd_exe() {
    let args = process_args(
        r"C:\Program Files\nodejs\npx.cmd",
        &["-y".to_string(), "fixture-package@1.0.0".to_string()],
        &[("AGENT_TOKEN".to_string(), "secret".to_string())],
        true,
    );

    assert_eq!(
        args,
        vec![
            "AGENT_TOKEN=secret",
            "cmd.exe",
            "/D",
            "/C",
            r"C:\Program Files\nodejs\npx.cmd",
            "-y",
            "fixture-package@1.0.0",
        ]
    );
}
