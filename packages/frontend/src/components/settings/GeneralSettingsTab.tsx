import {
  AlertTriangle,
  Bell,
  Bug,
  Check,
  Folder,
  Keyboard,
  Laptop,
  Moon,
  Sun,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import type {
  AppPreferencesRecord,
  ComposerSubmitShortcut,
  RuntimeSettingsResult,
} from "@openaide/app-shell-contracts";
import type { DesktopNotificationSettings } from "../../shells/webTaskNotifications";
import type { AppThemePreference, FrontendShellAppearance } from "../../services/frontendShell";
import { currentFrontendShell } from "../../services/frontendShell";
import { usesMobileComposerBehavior } from "../mobileComposerBehavior";
import { PopupDialog } from "../Popup";
import { DesktopRuntimeSettings } from "./DesktopRuntimeSettings";
import { SupportExportButton } from "../SupportExportDialog";

export function GeneralSettingsTab({
  backendConnection,
  developerSettingsUnlocked = false,
  desktopNotifications,
  appearance,
  onResetTaskHistory,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  preferences,
  runtimeSettings,
}: {
  backendConnection?: Pick<import("@openaide/app-server-client").BackendConnection, "request">;
  developerSettingsUnlocked?: boolean;
  desktopNotifications?: DesktopNotificationSettings;
  appearance?: FrontendShellAppearance;
  onResetTaskHistory?: () => Promise<void>;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  preferences: AppPreferencesRecord;
  runtimeSettings?: RuntimeSettingsResult;
}) {
  const mobileComposerBehavior = usesMobileComposerBehavior();
  const enterSends = preferences.composer_submit_shortcut === "enter";
  const newLineShortcut = enterSends ? "Ctrl/Cmd+Enter" : "Enter";
  const developerSettings = runtimeSettings?.developer;
  const desktopRuntime = currentFrontendShell()?.desktopRuntime;

  return (
    <div className="general-settings-panel">
      {appearance ? (
        <GeneralSection
          description="Theme changes apply immediately in this OpenAIDE app."
          label="Appearance"
        >
          <ThemePicker appearance={appearance} />
        </GeneralSection>
      ) : null}

      {desktopRuntime ? (
        <GeneralSection
          description="Choose the operating system that owns this OpenAIDE environment. Switching restarts the app."
          label="Environment"
        >
          <DesktopRuntimeSettings capability={desktopRuntime} />
        </GeneralSection>
      ) : null}

      {!mobileComposerBehavior ? (
        <GeneralSection label="Composer">
          <div className="general-preference-surface">
            <GeneralPreferenceRow
              action={(
                <span className="general-preference-action">
                  <kbd className="general-shortcut-key">{enterSends ? "Enter" : "Ctrl/Cmd+Enter"}</kbd>
                  <SettingsSwitch
                    checked={enterSends}
                    label="Send with Enter"
                    onChange={(checked) => onSetComposerSubmitShortcut(checked ? "enter" : "mod_enter")}
                  />
                </span>
              )}
              detail={`Press ${newLineShortcut} to add a new line.`}
              icon={<Keyboard size={17} />}
              label="Send with Enter"
            />
          </div>
        </GeneralSection>
      ) : null}

      {desktopNotifications && onSetDesktopNotifications ? (
        <GeneralSection label="Notifications">
          <div className="general-preference-surface">
            <GeneralPreferenceRow
              action={(
                <SettingsSwitch
                  checked={desktopNotifications.status === "enabled" || desktopNotifications.status === "blocked"}
                  disabled={desktopNotifications.status === "unsupported"}
                  label="Desktop notifications"
                  onChange={(checked) => { void onSetDesktopNotifications(checked); }}
                />
              )}
              detail={desktopNotificationDetail(desktopNotifications.status)}
              icon={<Bell size={17} />}
              label="Desktop notifications"
            />
          </div>
        </GeneralSection>
      ) : null}

      {developerSettings && developerSettingsUnlocked ? (
        <GeneralSection description="Local diagnostic controls for this App Server." label="Developer">
          <div className="general-preference-surface">
            <GeneralPreferenceRow
              action={(
                <SettingsSwitch
                  checked={developerSettings.acp_trace.enabled}
                  label="ACP logs"
                  onChange={onSetAcpTrace}
                />
              )}
              detail="Write ACP trace files for local debugging."
              icon={<Bug size={17} />}
              label="ACP logs"
            />
            <GeneralPreferenceRow
              action={(
                <code className="general-path-value" title={developerSettings.acp_trace.directory}>
                  {compactPathForSettings(developerSettings.acp_trace.directory)}
                </code>
              )}
              icon={<Folder size={17} />}
              label="Trace directory"
            />
          </div>
        </GeneralSection>
      ) : null}

      <GeneralSection description="Create a local troubleshooting bundle for a bug report." label="Support">
        <div className="general-preference-surface">
          <GeneralPreferenceRow action={<SupportExportButton connection={backendConnection} />} detail="Choose standard diagnostics, Agent sessions, and raw traces." icon={<Bug size={17} />} label="Diagnostics" />
        </div>
      </GeneralSection>

      {onResetTaskHistory ? (
        <GeneralSection description="Manage history stored on this device." label="Local data">
          <div className="general-danger-surface">
            <GeneralPreferenceRow
              action={<ResetTaskHistoryButton onReset={onResetTaskHistory} />}
              detail="Delete local tasks, chats, tool details, and recalled prompts."
              icon={<Trash2 size={16} />}
              label="Task history"
              tone="danger"
            />
          </div>
        </GeneralSection>
      ) : null}
    </div>
  );
}

function GeneralSection({
  children,
  description,
  label,
}: {
  children: ReactNode;
  description?: string;
  label: string;
}) {
  const headingId = `general-${label.toLowerCase().replaceAll(" ", "-")}`;
  return (
    <section className="general-settings-section" aria-labelledby={headingId}>
      <header className="general-settings-section-heading">
        <h2 id={headingId}>{label}</h2>
        {description ? <p>{description}</p> : null}
      </header>
      {children}
    </section>
  );
}

function ThemePicker({ appearance }: { appearance: FrontendShellAppearance }) {
  const [theme, setTheme] = useState<AppThemePreference>(() => appearance.theme());
  return (
    <div className="general-theme-grid" role="radiogroup" aria-label="App theme">
      {(["system", "light", "dark"] as const).map((choice) => {
        const Icon = choice === "system" ? Laptop : choice === "light" ? Sun : Moon;
        const selected = theme === choice;
        return (
          <button
            aria-checked={selected}
            className={`general-theme-choice ${selected ? "selected" : ""}`}
            key={choice}
            onClick={() => {
              appearance.setTheme(choice);
              setTheme(choice);
            }}
            role="radio"
            type="button"
          >
            {selected ? <span className="general-theme-check"><Check size={13} /></span> : null}
            <span className={`general-theme-preview ${choice}`} aria-hidden="true">
              <span className="general-theme-preview-sidebar" />
              <span className="general-theme-preview-main"><i /><i /><i /></span>
            </span>
            <span className="general-theme-label">
              <Icon size={14} />
              {choice[0].toUpperCase() + choice.slice(1)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function GeneralPreferenceRow({
  action,
  detail,
  icon,
  label,
  tone,
}: {
  action: ReactNode;
  detail?: string;
  icon: ReactNode;
  label: string;
  tone?: "danger";
}) {
  return (
    <div className="general-preference-row">
      <span className={`general-preference-icon ${tone ?? ""}`}>{icon}</span>
      <span className="general-preference-copy">
        <strong>{label}</strong>
        {detail ? <small>{detail}</small> : null}
      </span>
      {action}
    </div>
  );
}

function SettingsSwitch({
  checked,
  disabled = false,
  label,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange(checked: boolean): void;
}) {
  return (
    <label className="settings-switch" aria-label={label}>
      <input
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.checked)}
        type="checkbox"
      />
      <span className="settings-switch-track" aria-hidden="true" />
    </label>
  );
}

function ResetTaskHistoryButton({ onReset }: { onReset: () => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [error, setError] = useState<string>();
  const close = () => {
    if (resetting) return;
    setError(undefined);
    setOpen(false);
  };
  const confirm = async () => {
    setResetting(true);
    setError(undefined);
    try {
      await onReset();
      setOpen(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to reset Task history.");
    } finally {
      setResetting(false);
    }
  };

  return (
    <>
      <button
        aria-label="Reset task history"
        className="general-reset-history"
        onClick={() => setOpen(true)}
        type="button"
      >
        Reset history…
      </button>
      <PopupDialog
        className="settings-reset-dialog"
        label="Reset task history confirmation"
        onOpenChange={(nextOpen) => { if (!nextOpen) close(); }}
        open={open}
      >
        <header>
          <AlertTriangle size={17} />
          <div>
            <strong>Reset task history?</strong>
            <small>This cannot be undone.</small>
          </div>
        </header>
        <p>Permanently delete all OpenAIDE Tasks, chats, tool details, and recalled composer prompts from this device.</p>
        <p>Projects, files, worktrees, settings, credentials, and Agent-owned sessions are preserved. Agent-owned sessions may appear again in history.</p>
        {error ? <p className="settings-reset-error" role="alert">{error}</p> : null}
        <footer>
          <button disabled={resetting} onClick={close} type="button">Cancel</button>
          <button
            aria-label="Confirm reset task history"
            className="danger"
            disabled={resetting}
            onClick={() => void confirm()}
            type="button"
          >
            {resetting ? "Resetting…" : "Reset task history"}
          </button>
        </footer>
      </PopupDialog>
    </>
  );
}

function desktopNotificationDetail(status: DesktopNotificationSettings["status"]) {
  switch (status) {
    case "enabled":
      return "Show OS notifications when OpenAIDE is not focused.";
    case "blocked":
      return "Blocked by the browser or OS. Allow notifications in site settings.";
    case "unsupported":
      return "This browser does not support desktop notifications.";
    case "off":
      return "Show OS notifications when OpenAIDE is not focused.";
  }
}

export function compactPathForSettings(path: string, visibleSegments = 3): string {
  if (!path) return path;
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= visibleSegments) return path;
  const separator = path.includes("\\") ? "\\" : "/";
  return `...${separator}${parts.slice(-visibleSegments).join(separator)}`;
}
