//! The single schema-version boundary for OpenAIDE's ACP integration.
//!
//! Product code should import ACP wire types from this module so selecting a
//! protocol version remains an integration decision instead of leaking across
//! every projection and session service. The SDK pins the schema release; its
//! `v1::schema` is the protocol v1 message set.

pub use agent_client_protocol::schema::{MaybeUndefined, ProtocolVersion};
pub use agent_client_protocol::v1::schema::*;
