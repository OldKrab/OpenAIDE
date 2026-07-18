use super::AcpAgentConfig;

#[test]
fn codex_npx_fallback_uses_the_release_tested_exact_version() {
    let config = AcpAgentConfig::codex_npx_fallback();

    assert_eq!(config.command, "npx");
    assert_eq!(config.args, ["-y", "@agentclientprotocol/codex-acp@1.0.1"]);
}
