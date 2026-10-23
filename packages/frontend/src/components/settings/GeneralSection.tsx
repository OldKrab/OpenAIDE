import { Check, Monitor, SlidersHorizontal, TerminalSquare } from "lucide-react";
import { useState } from "react";
import type { AppPreferencesRecord, ComposerSubmitShortcut } from "@openaide/app-shell-contracts";
import type { DesktopNotificationSettings } from "../../shells/webTaskNotifications";
import {
  currentFrontendShell,
  type AppThemePreference,
  type DesktopRuntimeCapability,
  type DesktopRuntimeEnvironment,
  type FrontendShellAppearance,
} from "../../services/frontendShell";
import { usesMobileComposerBehavior } from "../mobileComposerBehavior";
import type { PhoneConnection } from "./phoneConnection";
import { settingsInfo } from "./settingsInfo";
import { SettingsButton, SettingsChoice, SettingsInlineError, SettingsKeys, SettingsNotice, SettingsSelect, SettingsSwitch } from "./settingsKit";
import type { SettingsGroup, SettingsRow, SettingsSection } from "./settingsModel";
import { useUpdateGroup } from "./updateSettings";

export type GeneralSectionProps = {
  appearance?: FrontendShellAppearance;
  replacementImpact?: () => Promise<import("@openaide/app-server-client").ClientReplacementImpactResult>;
  desktopNotifications?: DesktopNotificationSettings;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  phone?: PhoneConnection;
  preferences: AppPreferencesRecord;
};

/** Look, everyday behaviour, and what this device itself decides: its runtime, background work, updates. */
export function useGeneralSection({
  appearance,
  desktopNotifications,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  phone,
  preferences,
  replacementImpact,
}: GeneralSectionProps): SettingsSection {
  const shell = currentFrontendShell();
  // While a paired computer is in use, its own runtime applies and this choice has no effect.
  const runtime = useRuntimeGroup(shell?.remoteComputers?.current ? undefined : shell?.desktopRuntime);
  const updates = useUpdateGroup(shell?.desktopUpdates, replacementImpact);
  const enterSends = preferences.composer_submit_shortcut === "enter";
  const everyday: SettingsRow[] = [];
  if (!usesMobileComposerBehavior()) {
    everyday.push({
      id: "send-with-enter",
      label: "Send with Enter",
      keywords: "keyboard shortcut composer newline",
      description: enterSends ? "Shift+Enter adds a line" : `${modifierKey()}+Enter sends`,
      control: (
        <>
          <SettingsKeys keys={enterSends ? ["Enter"] : [modifierKey(), "Enter"]} />
          <SettingsSwitch
            checked={enterSends}
            label="Send with Enter"
            onChange={(checked) => onSetComposerSubmitShortcut(checked ? "enter" : "mod_enter")}
          />
        </>
      ),
    });
  }
  if (desktopNotifications && onSetDesktopNotifications) {
    everyday.push({
      id: "notifications",
      label: "Notifications",
      keywords: "alerts desktop os browser",
      description: notificationDetail(desktopNotifications.status),
      status: desktopNotifications.status === "blocked" ? { tone: "warn", text: "Blocked" } : undefined,
      control: (
        <SettingsSwitch
          checked={desktopNotifications.status === "enabled" || desktopNotifications.status === "blocked"}
          disabled={desktopNotifications.status === "unsupported"}
          label="Notifications"
          onChange={(checked) => { void onSetDesktopNotifications(checked); }}
        />
      ),
    });
  }

  const groups: SettingsGroup[] = [];
  if (appearance) {
    groups.push({
      id: "appearance",
      title: "Appearance",
      rows: [{ id: "theme", label: "Theme", keywords: "appearance dark light system color", below: <ThemePicker appearance={appearance} /> }],
    });
  }
  if (everyday.length) groups.push({ id: "everyday", title: "Everyday", rows: everyday });
  if (runtime) groups.push(runtime);
  const background = backgroundGroup(phone);
  if (background) groups.push(background);
  if (updates) groups.push(updates);
  return { id: "general", label: "General", icon: SlidersHorizontal, groups };
}

function ThemePicker({ appearance }: { appearance: FrontendShellAppearance }) {
  const [theme, setTheme] = useState<AppThemePreference>(() => appearance.theme());
  return (
    <div aria-label="Theme" className="settings-themes" role="radiogroup">
      {(["system", "light", "dark"] as const).map((choice) => (
        <button
          aria-checked={theme === choice}
          key={choice}
          onClick={() => {
            appearance.setTheme(choice);
            setTheme(choice);
          }}
          role="radio"
          type="button"
        >
          {/* The choice is shown as the thing it changes: a small window in that theme. */}
          <span aria-hidden="true" className={`settings-theme-window ${choice}`}><i /><b><u /><u /><u /></b></span>
          <span>{choice[0].toUpperCase() + choice.slice(1)}{theme === choice ? <Check aria-hidden="true" size={14} /> : null}</span>
        </button>
      ))}
    </div>
  );
}

/** Windows only: whether Windows or a WSL distribution owns projects, tools, and Agents. */
function useRuntimeGroup(capability: DesktopRuntimeCapability | undefined): SettingsGroup | undefined {
  const [distro, setDistro] = useState<string>();
  const [switching, setSwitching] = useState<DesktopRuntimeEnvironment>();
  const [failed, setFailed] = useState<DesktopRuntimeEnvironment["kind"]>();
  if (!capability) return undefined;
  const snapshot = capability.snapshot();
  const active = snapshot.active;
  const chosenDistro = distro ?? (active.kind === "wsl" ? active.distro : snapshot.wslDistros[0]);
  const select = async (environment: DesktopRuntimeEnvironment) => {
    if (switching) return;
    setFailed(undefined);
    setSwitching(environment);
    try {
      // The shell restarts to apply the choice, so a settled promise means it did not switch.
      await capability.select(environment);
    } catch {
      setFailed(environment.kind);
      setSwitching(undefined);
    }
  };
  const failure = "OpenAIDE could not switch. Try again.";
  const wslActive = active.kind === "wsl" && active.distro === chosenDistro;
  return {
    id: "runtime",
    title: "Agents run in",
    info: settingsInfo.runtime,
    rows: [
      {
        id: "windows",
        label: "Windows",
        keywords: "environment runtime native operating system",
        leading: <Monitor aria-hidden="true" size={18} strokeWidth={1.6} />,
        description: "Windows projects, tools and sign-ins",
        selected: active.kind === "native",
        error: failed === "native" ? failure : undefined,
        control: (
          <SettingsChoice
            active={active.kind === "native"}
            busy={switching?.kind === "native"}
            disabled={Boolean(switching)}
            label="Windows"
            onUse={() => void select({ kind: "native" })}
          />
        ),
      },
      {
        id: "wsl",
        label: "WSL",
        keywords: `linux environment runtime operating system ${snapshot.wslDistros.join(" ")}`,
        leading: <TerminalSquare aria-hidden="true" size={18} strokeWidth={1.6} />,
        description: snapshot.wslDistros.length
          ? "Linux projects, tools and sign-ins"
          : "Install an x64 WSL distribution to use it",
        selected: active.kind === "wsl",
        error: failed === "wsl" ? failure : undefined,
        control: snapshot.wslDistros.length ? (
          <>
            {snapshot.wslDistros.length > 1 ? (
              <SettingsSelect
                disabled={Boolean(switching)}
                label="WSL distribution"
                onChange={setDistro}
                options={snapshot.wslDistros.map((name) => ({ value: name, label: name }))}
                value={chosenDistro}
              />
            ) : <span className="settings-value">{chosenDistro}</span>}
            <SettingsChoice
              active={wslActive}
              busy={switching?.kind === "wsl"}
              disabled={Boolean(switching)}
              label="WSL"
              onUse={() => void select({ kind: "wsl", distro: chosenDistro })}
            />
          </>
        ) : undefined,
      },
    ],
  };
}

/** Phone only, and only while the phone itself runs the Agents. */
function backgroundGroup(phone: PhoneConnection | undefined): SettingsGroup | undefined {
  const state = phone?.state;
  if (!phone || !state || state.remote) return undefined;
  const rows: SettingsRow[] = [{
    id: "background",
    label: "Continue while locked",
    keywords: "background battery phone termux screen",
    control: (
      <SettingsSwitch
        checked={state.background}
        disabled={phone.busy}
        label="Continue while locked"
        onChange={(enabled) => { void phone.execute({ action: "background", enabled }); }}
      />
    ),
  }];
  if (state.background && (!state.appBattery || !state.termuxBattery)) {
    rows.push({
      id: "battery",
      label: "Allow background activity",
      description: "Unrestricted battery use for OpenAIDE and Termux",
      status: { tone: "warn", text: "Needed" },
      control: <SettingsButton disabled={phone.busy} onClick={() => void phone.execute({ action: "battery" })}>Open settings</SettingsButton>,
    });
  }
  if (state.background && !state.notifications) {
    rows.push({
      id: "work-notification",
      label: "Work notification",
      description: "See and stop background work",
      status: { tone: "warn", text: "Needed" },
      control: <SettingsButton disabled={phone.busy} onClick={() => void phone.execute({ action: "app_settings" })}>Allow</SettingsButton>,
    });
  }
  return {
    id: "background",
    title: "Background work",
    info: settingsInfo.background,
    notice: (
      <>
        {state.background && state.batterySaver ? <SettingsNotice>Battery Saver is on. Android may pause work.</SettingsNotice> : null}
        {phone.error ? <SettingsInlineError>{phone.error}</SettingsInlineError> : null}
      </>
    ),
    rows,
  };
}

function notificationDetail(status: DesktopNotificationSettings["status"]) {
  if (status === "blocked") return "Allow notifications for OpenAIDE in the browser or system settings";
  if (status === "unsupported") return "This browser does not support notifications";
  return "When a task finishes or needs you";
}

function modifierKey() {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform ?? "") ? "⌘" : "Ctrl";
}
