use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::{AbortHandle, Abortable};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};
use url::Url;

use crate::desktop_update_receipt::{
    ReceiptOutcome, UpdateAttemptReceipt, classify_receipt, read_receipt, write_receipt,
};
use crate::desktop_update_schedule::{auto_check_due, record_check_started, record_check_terminal};
use crate::desktop_update_security::{
    has_install_capacity, sha256_hex, trusted_redirect_url, validate_artifact_url,
};

const UPDATE_EVENT: &str = "desktop-update-snapshot";
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RELEASE_NOTES_BYTES: usize = 64 * 1024;
const UPDATE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub(crate) struct DesktopUpdateConfig {
    pub(crate) origin: Url,
    pub(crate) release_public_key: String,
    pub(crate) recovery_public_key: Option<String>,
}

impl DesktopUpdateConfig {
    fn for_build() -> Option<Self> {
        if cfg!(debug_assertions)
            || option_env!("OPENAIDE_DESKTOP_UPDATE_SIGNED_BUILD") != Some("1")
        {
            return None;
        }
        let origin = Url::parse(option_env!("OPENAIDE_DESKTOP_UPDATE_ORIGIN")?).ok()?;
        let release_public_key = option_env!("OPENAIDE_DESKTOP_UPDATE_RELEASE_PUBLIC_KEY")?;
        let recovery_public_key = option_env!("OPENAIDE_DESKTOP_UPDATE_RECOVERY_PUBLIC_KEY");
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || release_public_key.is_empty()
        {
            return None;
        }
        Some(Self {
            origin,
            release_public_key: release_public_key.to_string(),
            recovery_public_key: recovery_public_key.map(str::to_string),
        })
    }

    pub(crate) fn endpoint(&self, current_version: &str) -> Result<Url, DesktopUpdateError> {
        let version =
            Version::parse(current_version).map_err(|_| DesktopUpdateError::Configuration)?;
        let channel = if version.pre.is_empty() {
            "stable"
        } else {
            "prerelease"
        };
        let target = match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "windows",
            _ => return Err(DesktopUpdateError::UnsupportedInstallation),
        };
        let arch = match std::env::consts::ARCH {
            "aarch64" => "aarch64",
            "x86_64" => "x86_64",
            _ => return Err(DesktopUpdateError::UnsupportedInstallation),
        };
        self.origin
            .join(&format!(
                "v1/{channel}/{target}/{arch}/{current_version}.json"
            ))
            .map_err(|_| DesktopUpdateError::Configuration)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopUpdateSnapshot {
    revision: u64,
    installed_version: String,
    kind: DesktopUpdateKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_reason: Option<DesktopUpdateUnavailableReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offer: Option<DesktopUpdateOffer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<DesktopUpdateProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<DesktopUpdateError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_checked_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_version: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DesktopUpdateKind {
    Unavailable,
    Idle,
    Checking,
    Available,
    Downloading,
    ReadyToUpdate,
    Applying,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DesktopUpdateUnavailableReason {
    DevelopmentBuild,
    UnsignedBuild,
    NotConfigured,
    UnsupportedInstallation,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopUpdateOffer {
    version: String,
    notes: String,
    size_bytes: u64,
    published_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopUpdateProgress {
    downloaded_bytes: u64,
    total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DesktopUpdateError {
    Network,
    InvalidManifest,
    UntrustedArtifact,
    ArtifactTooLarge,
    InsufficientSpace,
    DownloadFailed,
    InstallFailed,
    Configuration,
    UnsupportedInstallation,
    IncompleteUpdate,
    ShutdownFailed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AbortReason {
    User,
    ArtifactTooLarge,
}

#[derive(Clone)]
struct Candidate {
    update: Update,
    offer: DesktopUpdateOffer,
    expected_sha256: String,
}

struct DesktopUpdateRuntime {
    snapshot: DesktopUpdateSnapshot,
    candidate: Option<Candidate>,
    bytes: Option<Vec<u8>>,
    download_abort: Option<AbortHandle>,
    abort_reason: Option<AbortReason>,
}

pub(crate) struct DesktopUpdateState {
    config: Option<DesktopUpdateConfig>,
    receipt_path: PathBuf,
    schedule_path: PathBuf,
    runtime: Mutex<DesktopUpdateRuntime>,
}

impl DesktopUpdateState {
    pub(crate) fn for_build(receipt_path: PathBuf, schedule_path: PathBuf) -> Self {
        let config = DesktopUpdateConfig::for_build();
        let unavailable_reason = if config.is_some() {
            None
        } else if cfg!(debug_assertions) {
            Some(DesktopUpdateUnavailableReason::DevelopmentBuild)
        } else if option_env!("OPENAIDE_DESKTOP_UPDATE_SIGNED_BUILD") != Some("1") {
            Some(DesktopUpdateUnavailableReason::UnsignedBuild)
        } else {
            Some(DesktopUpdateUnavailableReason::NotConfigured)
        };
        Self::new(config, receipt_path, schedule_path, unavailable_reason)
    }

    pub(crate) fn new(
        config: Option<DesktopUpdateConfig>,
        receipt_path: PathBuf,
        schedule_path: PathBuf,
        unavailable_reason: Option<DesktopUpdateUnavailableReason>,
    ) -> Self {
        let kind = if config.is_some() {
            DesktopUpdateKind::Idle
        } else {
            DesktopUpdateKind::Unavailable
        };
        Self {
            config,
            receipt_path,
            schedule_path,
            runtime: Mutex::new(DesktopUpdateRuntime {
                snapshot: DesktopUpdateSnapshot {
                    revision: 0,
                    installed_version: env!("CARGO_PKG_VERSION").to_string(),
                    kind,
                    unavailable_reason,
                    offer: None,
                    progress: None,
                    error: None,
                    last_checked_at_ms: None,
                    updated_version: None,
                },
                candidate: None,
                bytes: None,
                download_abort: None,
                abort_reason: None,
            }),
        }
    }

    pub(crate) fn snapshot(&self) -> Result<DesktopUpdateSnapshot, String> {
        self.runtime
            .lock()
            .map(|runtime| runtime.snapshot.clone())
            .map_err(|_| "Desktop Update state is unavailable.".to_string())
    }

    fn mutate(
        &self,
        app: &AppHandle,
        update: impl FnOnce(&mut DesktopUpdateRuntime),
    ) -> Result<DesktopUpdateSnapshot, String> {
        let snapshot = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| "Desktop Update state is unavailable.".to_string())?;
            update(&mut runtime);
            runtime.snapshot.revision += 1;
            runtime.snapshot.clone()
        };
        let _ = app.emit(UPDATE_EVENT, &snapshot);
        Ok(snapshot)
    }

    fn fail(
        &self,
        app: &AppHandle,
        error: DesktopUpdateError,
    ) -> Result<DesktopUpdateSnapshot, String> {
        self.mutate(app, |runtime| {
            runtime.snapshot.kind = DesktopUpdateKind::Failed;
            runtime.snapshot.error = Some(error);
            runtime.snapshot.progress = None;
            runtime.bytes = None;
            runtime.download_abort = None;
            runtime.abort_reason = None;
        })
    }
}

#[tauri::command]
pub(crate) fn desktop_update_snapshot(
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    state.snapshot()
}

#[tauri::command]
pub(crate) async fn desktop_check_for_update(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    let eligible = check_is_eligible(state.snapshot()?.kind);
    let result = check_for_update(app, state.inner()).await?;
    if eligible && state.config.is_some() {
        record_check_terminal(
            &state.schedule_path,
            unix_time_ms(),
            result.kind != DesktopUpdateKind::Failed,
        );
    }
    Ok(result)
}

#[tauri::command]
pub(crate) async fn desktop_auto_check_for_update(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    if state.config.is_none() || !check_is_eligible(state.snapshot()?.kind) {
        return state.snapshot();
    }
    let now = unix_time_ms();
    if !auto_check_due(&state.schedule_path, now) {
        return state.snapshot();
    }
    record_check_started(&state.schedule_path, now);
    let result = check_for_update(app.clone(), state.inner()).await?;
    let succeeded = result.kind != DesktopUpdateKind::Failed;
    record_check_terminal(&state.schedule_path, unix_time_ms(), succeeded);
    if succeeded {
        Ok(result)
    } else {
        // Automatic failures are diagnostic-only. Manual Check for Updates is
        // the explicit path that promotes a classified error into product UI.
        state.mutate(&app, |runtime| {
            runtime.snapshot.kind = DesktopUpdateKind::Idle;
            runtime.snapshot.error = None;
        })
    }
}

fn check_is_eligible(kind: DesktopUpdateKind) -> bool {
    matches!(
        kind,
        DesktopUpdateKind::Idle | DesktopUpdateKind::Available | DesktopUpdateKind::Failed
    )
}

async fn check_for_update(
    app: AppHandle,
    state: &DesktopUpdateState,
) -> Result<DesktopUpdateSnapshot, String> {
    let current = state.snapshot()?;
    if matches!(
        current.kind,
        DesktopUpdateKind::Unavailable
            | DesktopUpdateKind::Checking
            | DesktopUpdateKind::Downloading
            | DesktopUpdateKind::ReadyToUpdate
            | DesktopUpdateKind::Applying
    ) {
        return Ok(current);
    }
    let Some(config) = state.config.clone() else {
        return Ok(current);
    };
    state.mutate(&app, |runtime| {
        runtime.snapshot.kind = DesktopUpdateKind::Checking;
        runtime.snapshot.error = None;
        runtime.snapshot.updated_version = None;
    })?;

    let operation_id = format!("desktop-update-check-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    eprintln!("desktop_update_check_started operation_id={operation_id}");
    let update = match check_with_trusted_identity(&app, &config).await {
        Ok(update) => update,
        Err(error) => {
            log_terminal("check", &operation_id, "failure", started_at, Some(error));
            return state.fail(&app, error);
        }
    };

    let checked_at = unix_time_ms();
    let Some(update) = update else {
        log_terminal("check", &operation_id, "current", started_at, None);
        return state.mutate(&app, |runtime| {
            runtime.snapshot.kind = DesktopUpdateKind::Idle;
            runtime.snapshot.offer = None;
            runtime.snapshot.last_checked_at_ms = Some(checked_at);
            runtime.candidate = None;
            runtime.bytes = None;
        });
    };
    let candidate = match candidate_from_update(update) {
        Ok(candidate) => candidate,
        Err(error) => return state.fail(&app, error),
    };
    let offer = candidate.offer.clone();
    log_terminal("check", &operation_id, "available", started_at, None);
    state.mutate(&app, |runtime| {
        runtime.snapshot.kind = DesktopUpdateKind::Available;
        runtime.snapshot.offer = Some(offer);
        runtime.snapshot.last_checked_at_ms = Some(checked_at);
        runtime.snapshot.error = None;
        runtime.candidate = Some(candidate);
        runtime.bytes = None;
    })
}

#[tauri::command]
pub(crate) async fn desktop_download_update(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    let candidate = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Desktop Update state is unavailable.".to_string())?;
        if runtime.snapshot.kind != DesktopUpdateKind::Available {
            return Ok(runtime.snapshot.clone());
        }
        runtime.candidate.clone()
    };
    let Some(candidate) = candidate else {
        return state.fail(&app, DesktopUpdateError::InvalidManifest);
    };
    if !has_install_capacity(&state.receipt_path, candidate.offer.size_bytes) {
        return state.fail(&app, DesktopUpdateError::InsufficientSpace);
    }
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let oversize_abort = abort_handle.clone();
    state.mutate(&app, |runtime| {
        runtime.snapshot.kind = DesktopUpdateKind::Downloading;
        runtime.snapshot.error = None;
        runtime.snapshot.progress = Some(DesktopUpdateProgress {
            downloaded_bytes: 0,
            total_bytes: candidate.offer.size_bytes,
        });
        runtime.download_abort = Some(abort_handle);
        runtime.abort_reason = None;
    })?;

    let operation_id = format!("desktop-update-download-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    eprintln!(
        "desktop_update_download_started operation_id={operation_id} target_version={}",
        candidate.offer.version
    );
    let mut downloaded = 0_u64;
    let download = candidate.update.download(
        |chunk_size, _content_length| {
            downloaded = downloaded.saturating_add(chunk_size as u64);
            if downloaded > MAX_ARTIFACT_BYTES {
                if let Ok(mut runtime) = state.runtime.lock() {
                    runtime.abort_reason = Some(AbortReason::ArtifactTooLarge);
                }
                oversize_abort.abort();
                return;
            }
            let _ = state.mutate(&app, |runtime| {
                runtime.snapshot.progress = Some(DesktopUpdateProgress {
                    downloaded_bytes: downloaded.min(candidate.offer.size_bytes),
                    total_bytes: candidate.offer.size_bytes,
                });
            });
        },
        || {},
    );
    let result = Abortable::new(download, abort_registration).await;
    match result {
        Ok(Ok(bytes)) => {
            if bytes.len() as u64 != candidate.offer.size_bytes
                || sha256_hex(&bytes) != candidate.expected_sha256
            {
                log_terminal(
                    "download",
                    &operation_id,
                    "failure",
                    started_at,
                    Some(DesktopUpdateError::UntrustedArtifact),
                );
                return state.fail(&app, DesktopUpdateError::UntrustedArtifact);
            }
            log_terminal("download", &operation_id, "success", started_at, None);
            state.mutate(&app, |runtime| {
                runtime.snapshot.kind = DesktopUpdateKind::ReadyToUpdate;
                runtime.snapshot.progress = None;
                runtime.bytes = Some(bytes);
                runtime.download_abort = None;
                runtime.abort_reason = None;
            })
        }
        Ok(Err(_)) => {
            log_terminal(
                "download",
                &operation_id,
                "failure",
                started_at,
                Some(DesktopUpdateError::DownloadFailed),
            );
            state.fail(&app, DesktopUpdateError::DownloadFailed)
        }
        Err(_) => {
            let reason = state
                .runtime
                .lock()
                .ok()
                .and_then(|runtime| runtime.abort_reason);
            if reason == Some(AbortReason::ArtifactTooLarge) {
                return state.fail(&app, DesktopUpdateError::ArtifactTooLarge);
            }
            log_terminal("download", &operation_id, "cancelled", started_at, None);
            state.mutate(&app, |runtime| {
                runtime.snapshot.kind = DesktopUpdateKind::Available;
                runtime.snapshot.progress = None;
                runtime.download_abort = None;
                runtime.abort_reason = None;
                runtime.bytes = None;
            })
        }
    }
}

#[tauri::command]
pub(crate) fn desktop_cancel_update_download(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    let abort = {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Desktop Update state is unavailable.".to_string())?;
        if runtime.snapshot.kind != DesktopUpdateKind::Downloading {
            return Ok(runtime.snapshot.clone());
        }
        runtime.abort_reason = Some(AbortReason::User);
        runtime.download_abort.clone()
    };
    abort.map(|handle| handle.abort());
    state.snapshot()
}

#[tauri::command]
pub(crate) async fn desktop_install_update(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
    desktop_state: tauri::State<'_, crate::DesktopState>,
) -> Result<DesktopUpdateSnapshot, String> {
    let (candidate, bytes) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Desktop Update state is unavailable.".to_string())?;
        if runtime.snapshot.kind != DesktopUpdateKind::ReadyToUpdate {
            return Ok(runtime.snapshot.clone());
        }
        (runtime.candidate.clone(), runtime.bytes.clone())
    };
    let (Some(candidate), Some(bytes)) = (candidate, bytes) else {
        state.fail(&app, DesktopUpdateError::DownloadFailed)?;
        return Err("The downloaded update is unavailable.".to_string());
    };
    if sha256_hex(&bytes) != candidate.expected_sha256 {
        state.fail(&app, DesktopUpdateError::UntrustedArtifact)?;
        return Err("The downloaded update could not be verified.".to_string());
    }
    if !has_install_capacity(&state.receipt_path, candidate.offer.size_bytes) {
        state.fail(&app, DesktopUpdateError::InsufficientSpace)?;
        return Err("There is not enough storage to install the update.".to_string());
    }
    let Some(config) = state.config.clone() else {
        state.fail(&app, DesktopUpdateError::Configuration)?;
        return Err("Desktop Update is not configured.".to_string());
    };
    if let Err(error) = revalidate_candidate(&app, &config, &candidate).await {
        state.fail(&app, error)?;
        return Err(
            "The update offer changed, was withdrawn, or could not be revalidated.".to_string(),
        );
    }
    state.mutate(&app, |runtime| {
        runtime.snapshot.kind = DesktopUpdateKind::Applying;
        runtime.snapshot.error = None;
    })?;
    let endpoint_url = desktop_state.app_server_endpoint_url().map_err(|error| {
        let _ = state.fail(&app, DesktopUpdateError::ShutdownFailed);
        error
    })?;
    if crate::desktop_update_shutdown::wait_for_app_server_shutdown(endpoint_url)
        .await
        .is_err()
    {
        state.fail(&app, DesktopUpdateError::ShutdownFailed)?;
        return Err("The App Server did not shut down cleanly.".to_string());
    }
    if write_receipt(
        &state.receipt_path,
        &UpdateAttemptReceipt {
            previous_version: env!("CARGO_PKG_VERSION").to_string(),
            target_version: candidate.offer.version.clone(),
            attempted_at_ms: unix_time_ms(),
        },
    )
    .is_err()
    {
        state.fail(&app, DesktopUpdateError::InstallFailed)?;
        return Err("The update receipt could not be recorded.".to_string());
    }
    let operation_id = format!("desktop-update-install-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    eprintln!(
        "desktop_update_install_started operation_id={operation_id} target_version={}",
        candidate.offer.version
    );
    if candidate.update.install(&bytes).is_err() {
        log_terminal(
            "install",
            &operation_id,
            "failure",
            started_at,
            Some(DesktopUpdateError::InstallFailed),
        );
        state.fail(&app, DesktopUpdateError::InstallFailed)?;
        return Err("The platform installer could not apply the update.".to_string());
    }
    log_terminal("install", &operation_id, "handoff", started_at, None);
    app.restart();
}

#[tauri::command]
pub(crate) fn desktop_update_mark_interactive(
    app: AppHandle,
    state: tauri::State<'_, DesktopUpdateState>,
) -> Result<DesktopUpdateSnapshot, String> {
    let current = env!("CARGO_PKG_VERSION");
    match classify_receipt(current, read_receipt(&state.receipt_path).as_ref()) {
        ReceiptOutcome::None => state.snapshot(),
        ReceiptOutcome::Updated(version) => {
            let _ = std::fs::remove_file(&state.receipt_path);
            state.mutate(&app, |runtime| {
                runtime.snapshot.updated_version = Some(version);
            })
        }
        ReceiptOutcome::Incomplete => state.fail(&app, DesktopUpdateError::IncompleteUpdate),
    }
}

async fn check_with_key(
    app: &AppHandle,
    endpoint: Url,
    public_key: &str,
) -> Result<Option<Update>, DesktopUpdateError> {
    let feed_host = endpoint.host_str().unwrap_or_default().to_string();
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|_| DesktopUpdateError::Configuration)?
        .pubkey(public_key)
        .timeout(UPDATE_TIMEOUT)
        .configure_client(move |client| {
            let feed_host = feed_host.clone();
            client.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                if trusted_redirect_url(attempt.url(), &feed_host) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
        })
        .build()
        .map_err(|_| DesktopUpdateError::Configuration)?
        .check()
        .await
        .map_err(|_| DesktopUpdateError::Network)
}

async fn check_with_trusted_identity(
    app: &AppHandle,
    config: &DesktopUpdateConfig,
) -> Result<Option<Update>, DesktopUpdateError> {
    let mut endpoint = config.endpoint(env!("CARGO_PKG_VERSION"))?;
    endpoint
        .query_pairs_mut()
        .append_pair("check", &unix_time_ms().to_string());
    let mut update = check_with_key(app, endpoint.clone(), &config.release_public_key).await?;
    if update
        .as_ref()
        .and_then(signing_identity)
        .is_some_and(|identity| identity == "recovery")
    {
        let recovery_key = config
            .recovery_public_key
            .as_deref()
            .ok_or(DesktopUpdateError::InvalidManifest)?;
        update = match check_with_key(app, endpoint, recovery_key).await? {
            Some(candidate) if signing_identity(&candidate).as_deref() == Some("recovery") => {
                Some(candidate)
            }
            _ => return Err(DesktopUpdateError::InvalidManifest),
        };
    }
    Ok(update)
}

async fn revalidate_candidate(
    app: &AppHandle,
    config: &DesktopUpdateConfig,
    expected: &Candidate,
) -> Result<(), DesktopUpdateError> {
    let update = check_with_trusted_identity(app, config)
        .await?
        .ok_or(DesktopUpdateError::InvalidManifest)?;
    let fresh = candidate_from_update(update)?;
    let unchanged = fresh.offer.version == expected.offer.version
        && fresh.offer.size_bytes == expected.offer.size_bytes
        && fresh.expected_sha256 == expected.expected_sha256
        && fresh.update.download_url == expected.update.download_url
        && fresh.update.signature == expected.update.signature
        && signing_identity(&fresh.update) == signing_identity(&expected.update);
    unchanged
        .then_some(())
        .ok_or(DesktopUpdateError::InvalidManifest)
}

fn signing_identity(update: &Update) -> Option<String> {
    update
        .raw_json
        .get("signingIdentity")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn candidate_from_update(update: Update) -> Result<Candidate, DesktopUpdateError> {
    let identity = signing_identity(&update).ok_or(DesktopUpdateError::InvalidManifest)?;
    if identity != "release" && identity != "recovery" {
        return Err(DesktopUpdateError::InvalidManifest);
    }
    validate_artifact_url(&update.download_url)?;
    let size_bytes = update
        .raw_json
        .get("artifactSize")
        .and_then(serde_json::Value::as_u64)
        .filter(|size| *size > 0 && *size <= MAX_ARTIFACT_BYTES)
        .ok_or(DesktopUpdateError::ArtifactTooLarge)?;
    let expected_sha256 = update
        .raw_json
        .get("artifactSha256")
        .and_then(serde_json::Value::as_str)
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or(DesktopUpdateError::InvalidManifest)?
        .to_ascii_lowercase();
    let notes = update.body.clone().unwrap_or_default();
    if notes.len() > MAX_RELEASE_NOTES_BYTES {
        return Err(DesktopUpdateError::InvalidManifest);
    }
    let offer = DesktopUpdateOffer {
        version: update.version.clone(),
        notes,
        size_bytes,
        published_at: update.date.map(|date| date.to_string()),
    };
    Ok(Candidate {
        update,
        offer,
        expected_sha256,
    })
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn log_terminal(
    operation: &str,
    operation_id: &str,
    outcome: &str,
    started_at: Instant,
    error: Option<DesktopUpdateError>,
) {
    let error = error
        .map(|error| format!(" error_kind={}", format!("{error:?}").to_ascii_lowercase()))
        .unwrap_or_default();
    eprintln!(
        "desktop_update_{operation}_completed operation_id={operation_id} outcome={outcome} duration_ms={}{}",
        started_at.elapsed().as_millis(),
        error
    );
}
