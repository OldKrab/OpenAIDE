//! An Own Relay server (ADR-0085): an iroh relay that admits each person by their own
//! relay key, with a management API that OpenAIDE's App Server calls.
//!
//! - [`keys`] owns who may use the relay; [`access`] enforces it on every relay connection.
//! - [`api`] is the management API; [`serve`] assembles the running relay.
//! - [`tls`] and [`acme`] own the certificate; [`setup`] installs the service.

pub mod access;
pub mod acme;
pub mod api;
pub mod cli;
pub mod config;
pub mod files;
pub mod keys;
pub mod log;
pub mod probe;
pub mod serve;
pub mod setup;
pub mod tls;
