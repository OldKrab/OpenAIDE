use std::path::Path;

use sha2::{Digest, Sha256};
use url::Url;

use crate::desktop_update::DesktopUpdateError;

const RELEASE_DOWNLOAD_PREFIX: &str = "/OldKrab/OpenAIDE/releases/download/";

pub(crate) fn validate_artifact_url(url: &Url) -> Result<(), DesktopUpdateError> {
    let trusted = url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.path().starts_with(RELEASE_DOWNLOAD_PREFIX)
        && url.username().is_empty()
        && url.password().is_none();
    trusted
        .then_some(())
        .ok_or(DesktopUpdateError::UntrustedArtifact)
}

pub(crate) fn trusted_redirect_url(url: &Url, feed_host: &str) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(host)
                if host == feed_host
                    || host == "github.com"
                    || host == "objects.githubusercontent.com"
                    || host == "release-assets.githubusercontent.com"
        )
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn has_install_capacity(state_path: &Path, artifact_size: u64) -> bool {
    let Some(mut capacity_root) = state_path.parent() else {
        return false;
    };
    while !capacity_root.exists() {
        let Some(parent) = capacity_root.parent() else {
            return false;
        };
        capacity_root = parent;
    }
    // The installer and its extraction may coexist. Keep a fixed margin for
    // platform metadata and the bundled App Server.
    let required = artifact_size
        .saturating_mul(2)
        .saturating_add(128 * 1024 * 1024);
    fs2::available_space(capacity_root).is_ok_and(|available| available >= required)
}
