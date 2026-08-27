use super::AcpAgentConfig;
use super::{
    command_not_found_error, process_args, resolve_command_in_paths, windows_command_extensions,
};
use crate::protocol::errors::RuntimeError;
use std::ffi::OsStr;
use std::fs;

#[test]
fn built_in_codex_uses_the_product_pinned_adapter() {
    let config = AcpAgentConfig::codex_npx_fallback();

    assert_eq!(config.agent_id, "codex");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args, ["-y", "@openaide/codex-acp@1.1.0"]);
    assert_eq!(config.diagnostic_launcher_kind(), "pinned_npx_package");
}

#[test]
fn missing_codex_npx_is_classified_as_node_js_required() {
    assert!(matches!(
        command_not_found_error("codex", "npx"),
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
        &["-y".to_string(), "@openaide/codex-acp@1.1.0".to_string()],
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
            "@openaide/codex-acp@1.1.0",
        ]
    );
}
