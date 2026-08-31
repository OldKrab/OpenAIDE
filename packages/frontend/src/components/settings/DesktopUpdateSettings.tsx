import { CheckCircle2, Download, RefreshCw, RotateCw, X } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import Markdown from "react-markdown";
import type {
  DesktopUpdateCapability,
  DesktopUpdateRestartResult,
  DesktopUpdateSnapshot,
} from "../../services/frontendShell";

export function DesktopUpdateSettings({ capability }: { capability: DesktopUpdateCapability }) {
  const snapshot = useSyncExternalStore(
    capability.subscribe,
    capability.snapshot,
    capability.snapshot,
  );
  const [actionError, setActionError] = useState(false);
  const [deferred, setDeferred] = useState(false);
  const [restartBlocker, setRestartBlocker] = useState<Exclude<DesktopUpdateRestartResult, "started">>();
  const run = async (action: () => Promise<void>) => {
    setActionError(false);
    try {
      await action();
    } catch {
      setActionError(true);
    }
  };
  const offer = snapshot.offer;
  const status = updateStatus(snapshot);
  const restart = async (stopActiveWork = false) => {
    setActionError(false);
    try {
      const result = await capability.restartAndUpdate({ stopActiveWork });
      setRestartBlocker(result === "started" ? undefined : result);
    } catch {
      setActionError(true);
    }
  };

  return (
    <div className="desktop-update-settings" aria-live="polite">
      {snapshot.updatedVersion ? (
        <p className="desktop-update-success">
          <CheckCircle2 aria-hidden="true" size={16} />
          <span>Updated to version {snapshot.updatedVersion}.</span>
          <button onClick={() => capability.openReleaseNotes(snapshot.updatedVersion!)} type="button">
            View what&apos;s new
          </button>
        </p>
      ) : null}

      <div className="desktop-update-summary">
        <span className="desktop-update-copy">
          <strong>{status.label}</strong>
          <small>{status.detail}</small>
        </span>
        <UpdateAction
          capability={capability}
          deferred={deferred}
          onDefer={() => setDeferred(true)}
          onResume={() => setDeferred(false)}
          restart={restart}
          restartBlocker={restartBlocker}
          clearRestartBlocker={() => setRestartBlocker(undefined)}
          run={run}
          snapshot={snapshot}
        />
      </div>

      {snapshot.kind === "downloading" && snapshot.progress ? (
        <progress
          aria-label={`Downloading OpenAIDE ${offer?.version ?? "update"}`}
          max={snapshot.progress.totalBytes}
          value={snapshot.progress.downloadedBytes}
        />
      ) : null}

      {snapshot.kind === "readyToUpdate" && restartBlocker ? (
        <p className="desktop-update-shutdown-blocker" role="status">
          {restartBlocker === "activeWork"
            ? "OpenAIDE is still working. Updating now will stop active work before restarting."
            : "Another OpenAIDE client is using this App Server. Close it before updating."}
        </p>
      ) : null}

      {offer && !deferred ? (
        <details className="desktop-update-notes">
          <summary>Release notes</summary>
          <div className="desktop-update-notes-body">
            <Markdown
              allowedElements={["p", "h1", "h2", "h3", "ul", "ol", "li", "pre", "code", "strong", "em"]}
              skipHtml
              unwrapDisallowed
            >
              {offer.notes || "No release notes were provided."}
            </Markdown>
          </div>
        </details>
      ) : null}

      {actionError ? (
        <p className="desktop-update-action-error" role="alert">
          OpenAIDE could not complete that update action. Try again.
        </p>
      ) : null}
    </div>
  );
}

function UpdateAction({
  capability,
  deferred,
  onDefer,
  onResume,
  restart,
  restartBlocker,
  clearRestartBlocker,
  run,
  snapshot,
}: {
  capability: DesktopUpdateCapability;
  deferred: boolean;
  onDefer(): void;
  onResume(): void;
  restart(stopActiveWork?: boolean): Promise<void>;
  restartBlocker: Exclude<DesktopUpdateRestartResult, "started"> | undefined;
  clearRestartBlocker(): void;
  run(action: () => Promise<void>): Promise<void>;
  snapshot: DesktopUpdateSnapshot;
}) {
  if (snapshot.kind === "unavailable") return null;
  if (snapshot.kind === "checking") {
    return <button className="settings-secondary-button" disabled type="button"><RefreshCw size={14} />Checking</button>;
  }
  if (snapshot.kind === "downloading") {
    return (
      <button className="settings-secondary-button" onClick={() => void run(capability.cancelDownload)} type="button">
        <X size={14} />Cancel
      </button>
    );
  }
  if (snapshot.kind === "applying") {
    return <button className="settings-primary-button" disabled type="button"><RotateCw size={14} />Applying</button>;
  }
  if (snapshot.kind === "available") {
    return (
      <button className="settings-primary-button" onClick={() => void run(capability.download)} type="button">
        <Download size={14} />Download update
      </button>
    );
  }
  if (snapshot.kind === "readyToUpdate") {
    if (deferred) {
      return <button className="settings-secondary-button" onClick={onResume} type="button">Show update</button>;
    }
    if (restartBlocker === "activeWork") {
      return (
        <span className="desktop-update-actions">
          <button className="settings-secondary-button" onClick={clearRestartBlocker} type="button">Cancel</button>
          <button className="settings-primary-button" onClick={() => void restart(true)} type="button">
            <RotateCw size={14} />Stop work and update
          </button>
        </span>
      );
    }
    if (restartBlocker === "otherClients") {
      return (
        <button className="settings-secondary-button" onClick={() => void restart()} type="button">
          <RefreshCw size={14} />Try again
        </button>
      );
    }
    return (
      <span className="desktop-update-actions">
        <button className="settings-secondary-button" onClick={onDefer} type="button">Not now</button>
        <button className="settings-primary-button" onClick={() => void restart()} type="button">
          <RotateCw size={14} />Restart and update
        </button>
      </span>
    );
  }
  return (
    <button className="settings-secondary-button" onClick={() => void run(capability.check)} type="button">
      <RefreshCw size={14} />{snapshot.kind === "failed" ? "Retry" : "Check for updates"}
    </button>
  );
}

function updateStatus(snapshot: DesktopUpdateSnapshot): { label: string; detail: string } {
  const installed = snapshot.installedVersion ? `Version ${snapshot.installedVersion}` : "OpenAIDE Desktop";
  if (snapshot.kind === "unavailable") {
    const reason = snapshot.unavailableReason === "developmentBuild"
      ? "Updates are disabled in development builds."
      : snapshot.unavailableReason === "unsignedBuild"
        ? "Unsigned builds must be updated with a new installer."
      : snapshot.unavailableReason === "unsupportedInstallation"
        ? "This installation must be updated with a new installer."
        : "Updates are not configured for this build.";
    return { label: installed, detail: reason };
  }
  if (snapshot.kind === "checking") return { label: "Checking for updates", detail: installed };
  if (snapshot.kind === "available" && snapshot.offer) {
    return {
      label: `Version ${snapshot.offer.version} is available`,
      detail: `${formatBytes(snapshot.offer.sizeBytes)} download. ${installed} is installed.`,
    };
  }
  if (snapshot.kind === "downloading" && snapshot.progress) {
    return {
      label: `Downloading version ${snapshot.offer?.version ?? "update"}`,
      detail: `${formatBytes(snapshot.progress.downloadedBytes)} of ${formatBytes(snapshot.progress.totalBytes)}`,
    };
  }
  if (snapshot.kind === "readyToUpdate") {
    return { label: `Version ${snapshot.offer?.version ?? "update"} is ready`, detail: "Restart OpenAIDE to apply it." };
  }
  if (snapshot.kind === "applying") return { label: "Applying update", detail: "OpenAIDE will reopen automatically." };
  if (snapshot.kind === "failed") return { label: "Update needs attention", detail: updateErrorCopy(snapshot.error) };
  const lastCheck = snapshot.lastCheckedAtMs
    ? ` Last checked ${new Date(snapshot.lastCheckedAtMs).toLocaleString()}.`
    : "";
  return { label: "OpenAIDE is up to date", detail: `${installed}.${lastCheck}` };
}

function updateErrorCopy(error: DesktopUpdateSnapshot["error"]): string {
  if (error === "untrustedArtifact" || error === "invalidManifest") return "OpenAIDE could not verify the offered update.";
  if (error === "artifactTooLarge") return "The offered update is larger than OpenAIDE permits.";
  if (error === "insufficientSpace") return "Free some storage, then try again.";
  if (error === "incompleteUpdate") return "The previous update did not complete. Retry or download the installer.";
  if (error === "shutdownFailed") return "OpenAIDE could not stop its App Server cleanly. The update was not installed.";
  if (error === "unsupportedInstallation") return "Download a new installer to update this copy.";
  return "OpenAIDE could not reach or apply the update. Your current version is unchanged.";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${Math.round(bytes / (1024 * 1024))} MB`;
}
