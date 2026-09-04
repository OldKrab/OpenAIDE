use super::validate_key;

#[test]
fn accepts_only_openaide_owned_printable_credential_keys() {
    assert!(validate_key("openaide.agent.codex.auth.api-key.env.OPENAI_API_KEY").is_ok());
    assert!(validate_key("openaide.mcp.server.header.Authorization").is_ok());
    assert!(validate_key("other.product.secret").is_err());
    assert!(validate_key("openaide.agent../escape").is_err());
    assert!(validate_key("openaide.agent.bad\nkey").is_err());
}
