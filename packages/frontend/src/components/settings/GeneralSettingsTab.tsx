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
import { DesktopUpdateSettings } from "./DesktopUpdateSettings";
import { SupportExportButton } from "../SupportExportDialog";

export function GeneralSettingsTab({
  desktopNotifications,
  appearance,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  preferences,
}: GeneralSettingsTabProps) {
  const mobileComposerBehavior = usesMobileComposerBehavior();
  const enterSends = preferences.composer_submit_shortcut === "enter";
  const newLineShortcut = enterSends ? "Ctrl/Cmd+Enter" : "Enter";

  return (
    <div className="general-settings-panel">
      {appearance ? (
        <GeneralSection id="settings-general-appearance" label="Appearance">
          <ThemePicker appearance={appearance} />
        </GeneralSection>
      ) : null}

      {!mobileComposerBehavior || (desktopNotifications && onSetDesktopNotifications) ? (
        <GeneralSection id="settings-general-behavior" label="Behavior">
          <div className="general-preference-surface">
            {!mobileComposerBehavior ? (
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
                detail={`Use ${newLineShortcut} for a new line.`}
                icon={<Keyboard size={17} />}
                label="Send with Enter"
              />
            ) : null}
            {desktopNotifications && onSetDesktopNotifications ? (
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
            ) : null}
          </div>
        </GeneralSection>
      ) : null}
    </div>
  );
}

type GeneralSettingsTabProps = {
  desktopNotifications?: DesktopNotificationSettings;
  appearance?: FrontendShellAppearance;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  preferences: AppPreferencesRecord;
};

export function DesktopSettingsTab() {
  const desktopRuntime = currentFrontendShell()?.desktopRuntime;
  const desktopUpdates = currentFrontendShell()?.desktopUpdates;

  return (
    <div className="general-settings-panel">
      {desktopRuntime ? (
        <GeneralSection
          description="Choose whether Windows or WSL owns projects, tools, and Agents. Switching restarts OpenAIDE."
          id="settings-desktop-environment"
          label="Environment"
        >
          <DesktopRuntimeSettings capability={desktopRuntime} />
        </GeneralSection>
      ) : null}
      {desktopUpdates ? (
        <GeneralSection id="settings-desktop-updates" label="Application updates">
          <DesktopUpdateSettings capability={desktopUpdates} />
        </GeneralSection>
      ) : null}
    </div>
  );
}

export function DataSupportSettingsTab({
  backendConnection,
  developerSettingsUnlocked = false,
  onResetTaskHistory,
  onSetAcpTrace,
  runtimeSettings,
}: {
  backendConnection?: Pick<import("@openaide/app-server-client").BackendConnection, "request">;
  developerSettingsUnlocked?: boolean;
  onResetTaskHistory?: () => Promise<void>;
  onSetAcpTrace: (enabled: boolean) => void;
  runtimeSettings?: RuntimeSettingsResult;
}) {
  const developerSettings = runtimeSettings?.developer;

  return (
    <div className="general-settings-panel">
      <GeneralSection id="settings-data-support" label="Support">
        <div className="general-preference-surface">
          <GeneralPreferenceRow action={<SupportExportButton connection={backendConnection} />} detail="Export diagnostics, Agent sessions, and raw traces for troubleshooting." icon={<Bug size={17} />} label="Diagnostics" />
        </div>
      </GeneralSection>

      {developerSettings && developerSettingsUnlocked ? (
        <GeneralSection description="Local controls for investigating App Server behavior." id="settings-data-developer" label="Developer">
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

      {onResetTaskHistory ? (
        <GeneralSection description="Destructive actions only affect data stored on this device." id="settings-data-local" label="Local data">
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
  id,
  label,
}: {
  children: ReactNode;
  description?: string;
  id: string;
  label: string;
}) {
  const headingId = `${id}-heading`;
  return (
    <section className="general-settings-section" aria-labelledby={headingId} id={id} tabIndex={-1}>
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
