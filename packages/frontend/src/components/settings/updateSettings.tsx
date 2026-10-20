import { Download, RotateCw } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import Markdown from "react-markdown";
import type {
  DesktopUpdateCapability,
  DesktopUpdateRestartResult,
  DesktopUpdateSnapshot,
} from "../../services/frontendShell";
import { SettingsButton } from "./settingsKit";
import type { SettingsGroup, SettingsRow, SettingsTone } from "./settingsModel";

const noSubscription = () => () => undefined;
const noSnapshot = () => undefined;

/**
 * The installed version as one row whose status and action follow the native
 * updater's state. The shell owns the update lifecycle; this only renders it.
 */
export function useUpdateGroup(capability: DesktopUpdateCapability | undefined): SettingsGroup | undefined {
  const snapshot = useSyncExternalStore(
    capability?.subscribe ?? noSubscription,
    capability?.snapshot ?? noSnapshot,
    capability?.snapshot ?? noSnapshot,
  );
  const [actionError, setActionError] = useState(false);
  const [deferred, setDeferred] = useState(false);
  const [restartBlocker, setRestartBlocker] = useState<Exclude<DesktopUpdateRestartResult, "started">>();
  if (!capability || !snapshot) return undefined;

  const run = async (action: () => Promise<void>) => {
    setActionError(false);
    try {
      await action();
    } catch {
      setActionError(true);
    }
  };
  const restart = async (stopActiveWork = false) => {
    setActionError(false);
    try {
      const result = await capability.restartAndUpdate({ stopActiveWork });
      setRestartBlocker(result === "started" ? undefined : result);
    } catch {
      setActionError(true);
    }
  };
  const status = updateStatus(snapshot, capability.installation);
  const offer = snapshot.offer;
  const ready = snapshot.kind === "readyToUpdate";

  const row: SettingsRow = {
    id: "update",
    label: snapshot.installedVersion ? `OpenAIDE ${snapshot.installedVersion}` : "OpenAIDE",
    keywords: "update version release restart download",
    leading: <span aria-hidden="true" className="settings-product-mark" />,
    status: { tone: status.tone, text: status.label },
    description: status.detail,
    control: (
      <UpdateAction
        capability={capability}
        clearRestartBlocker={() => setRestartBlocker(undefined)}
        deferred={deferred}
        onDefer={() => setDeferred(true)}
        onResume={() => setDeferred(false)}
        restart={restart}
        restartBlocker={restartBlocker}
        run={run}
        snapshot={snapshot}
      />
    ),
    error: actionError ? "OpenAIDE could not complete that update action. Try again." : undefined,
    below: (
      <>
        {snapshot.kind === "downloading" && snapshot.progress ? (
          <progress
            aria-label={`Downloading OpenAIDE ${offer?.version ?? "update"}`}
            className="settings-progress"
            max={snapshot.progress.totalBytes}
            value={snapshot.progress.downloadedBytes}
          />
        ) : null}
        {ready && restartBlocker ? (
          <p className="settings-row-note" role="status">
            {restartBlocker === "activeWork"
              ? "OpenAIDE is still working. Updating now will stop active work before restarting."
              : "Another OpenAIDE client is using this App Server. Close it before updating."}
          </p>
        ) : null}
        {snapshot.updatedVersion ? (
          <p className="settings-row-note">
            Updated to version {snapshot.updatedVersion}.{" "}
            <button onClick={() => void run(() => capability.openReleaseNotes(snapshot.updatedVersion!))} type="button">
              View what&apos;s new
            </button>
          </p>
        ) : null}
        {offer && !deferred ? (
          <details className="settings-release-notes">
            <summary>Release notes</summary>
            <div>
              <Markdown
                allowedElements={["p", "h1", "h2", "h3", "ul", "ol", "li", "pre", "code", "strong", "em", "a"]}
                components={{
                  a: ({ children, href }) => (
                    <a href={href} rel="noreferrer" target="_blank">{children}</a>
                  ),
                }}
                skipHtml
                unwrapDisallowed
              >
                {offer.notes || "No release notes were provided."}
              </Markdown>
            </div>
          </details>
        ) : null}
      </>
    ),
  };
  return { id: "updates", title: "Updates", rows: [row] };
}

function UpdateAction({
  capability,
  clearRestartBlocker,
  deferred,
  onDefer,
  onResume,
  restart,
  restartBlocker,
  run,
  snapshot,
}: {
  capability: DesktopUpdateCapability;
  clearRestartBlocker(): void;
  deferred: boolean;
  onDefer(): void;
  onResume(): void;
  restart(stopActiveWork?: boolean): Promise<void>;
  restartBlocker: Exclude<DesktopUpdateRestartResult, "started"> | undefined;
  run(action: () => Promise<void>): Promise<void>;
  snapshot: DesktopUpdateSnapshot;
}) {
  if (snapshot.kind === "unavailable") return null;
  if (snapshot.kind === "checking") return <SettingsButton disabled>Checking…</SettingsButton>;
  if (snapshot.kind === "downloading") {
    return <SettingsButton onClick={() => void run(capability.cancelDownload)}>Cancel</SettingsButton>;
  }
  if (snapshot.kind === "applying") return <SettingsButton disabled variant="primary">Applying…</SettingsButton>;
  if (snapshot.kind === "available") {
    return (
      <SettingsButton icon={<Download aria-hidden="true" size={14} />} onClick={() => void run(capability.download)} variant="primary">
        Download update
      </SettingsButton>
    );
  }
  if (snapshot.kind === "readyToUpdate") {
    if (deferred) return <SettingsButton onClick={onResume}>Show update</SettingsButton>;
    if (restartBlocker === "activeWork") {
      return (
        <>
          <SettingsButton onClick={clearRestartBlocker} variant="quiet">Cancel</SettingsButton>
          <SettingsButton icon={<RotateCw aria-hidden="true" size={14} />} onClick={() => void restart(true)} variant="primary">
            Stop work and update
          </SettingsButton>
        </>
      );
    }
    if (restartBlocker === "otherClients") return <SettingsButton onClick={() => void restart()}>Try again</SettingsButton>;
    return (
      <>
        <SettingsButton onClick={onDefer} variant="quiet">Not now</SettingsButton>
        <SettingsButton icon={<RotateCw aria-hidden="true" size={14} />} onClick={() => void restart()} variant="primary">
          {capability.installation === "systemInstaller" ? "Install update" : "Restart and update"}
        </SettingsButton>
      </>
    );
  }
  return (
    <SettingsButton onClick={() => void run(capability.check)}>
      {snapshot.kind === "failed" ? "Retry" : "Check now"}
    </SettingsButton>
  );
}

export function updateStatus(
  snapshot: DesktopUpdateSnapshot,
  installation: DesktopUpdateCapability["installation"] = "restart",
): { tone: SettingsTone; label: string; detail?: string } {
  const systemInstaller = installation === "systemInstaller";
  if (snapshot.kind === "unavailable") {
    const reason = snapshot.unavailableReason === "developmentBuild"
      ? "Updates are disabled in development builds."
      : snapshot.unavailableReason === "unsupportedInstallation"
        ? "This installation must be updated with a new installer."
        : "Updates are not configured for this build.";
    return { tone: "off", label: "No updates", detail: reason };
  }
  if (snapshot.kind === "checking") return { tone: "busy", label: "Checking for updates" };
  if (snapshot.kind === "available" && snapshot.offer) {
    return {
      tone: "warn",
      label: `Version ${snapshot.offer.version} is available`,
      detail: `${formatBytes(snapshot.offer.sizeBytes)} download`,
    };
  }
  if (snapshot.kind === "downloading" && snapshot.progress) {
    return {
      tone: "busy",
      label: `Downloading version ${snapshot.offer?.version ?? "update"}`,
      detail: `${formatBytes(snapshot.progress.downloadedBytes)} of ${formatBytes(snapshot.progress.totalBytes)}`,
    };
  }
  if (snapshot.kind === "readyToUpdate") {
    return {
      tone: "warn",
      label: `Version ${snapshot.offer?.version ?? "update"} is ready`,
      detail: systemInstaller ? "Install it when you are ready. OpenAIDE closes while it installs." : "Restart OpenAIDE to apply it.",
    };
  }
  if (snapshot.kind === "applying") {
    return {
      tone: "busy",
      label: "Applying update",
      detail: systemInstaller ? "Confirm in the installer if asked, then open OpenAIDE again." : "OpenAIDE will reopen automatically.",
    };
  }
  if (snapshot.kind === "failed") return { tone: "warn", label: "Update needs attention", detail: updateErrorCopy(snapshot.error) };
  return {
    tone: "ok",
    label: "Up to date",
    detail: snapshot.lastCheckedAtMs ? `Last checked ${new Date(snapshot.lastCheckedAtMs).toLocaleString()}` : undefined,
  };
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
