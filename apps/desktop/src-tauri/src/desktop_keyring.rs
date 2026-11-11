//! The platform credential store behind Desktop secrets and the device key.
//!
//! Entries written by `keyring` 3 must stay readable, because a missing entry reads as
//! "never saved": a lost device key silently unpairs this Desktop. Both stores below are
//! built with their defaults, which keep keyring 3's layout: on macOS a generic password
//! with the same service and account in the user's login keychain, and on Windows a
//! generic credential whose target name is `{user}.{service}`.
//! Do not switch macOS to the `protected` store or change the Windows delimiters
//! without migrating existing entries.

use keyring_core::api::CredentialStoreApi;
use keyring_core::{Entry, Error, Result};

/// The entry for `service` and `user` in this platform's credential store.
pub(crate) fn entry(service: &str, user: &str) -> Result<Entry> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new()?;
    store.build(service, user, None)
}

/// A metadata-only class of a credential store failure, safe to log.
pub(crate) fn error_class(error: &Error) -> &'static str {
    match error {
        Error::PlatformFailure(_) => "platform_failure",
        Error::NoStorageAccess(_) => "no_storage_access",
        Error::NoEntry => "no_entry",
        Error::BadEncoding(_) => "bad_encoding",
        Error::BadDataFormat(..) => "bad_data_format",
        Error::BadStoreFormat(_) => "bad_store_format",
        Error::TooLong(..) => "too_long",
        Error::Invalid(..) => "invalid",
        Error::Ambiguous(_) => "ambiguous",
        Error::NoDefaultStore => "no_default_store",
        Error::NotSupportedByStore(_) => "not_supported_by_store",
        _ => "other",
    }
}
