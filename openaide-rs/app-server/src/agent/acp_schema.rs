//! The single schema-version boundary for OpenAIDE's ACP integration.
//!
//! Product code should import ACP wire types from this module so selecting a
//! protocol version remains an integration decision instead of leaking across
//! every projection and session service.

pub use agent_client_protocol::schema::v1::*;
pub use agent_client_protocol::schema::{MaybeUndefined, ProtocolVersion};
