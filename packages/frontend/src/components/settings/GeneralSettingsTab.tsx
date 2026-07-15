import { Search, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import type {
  AppPreferencesRecord,
  ComposerSubmitShortcut,
  RuntimeSettingsResult,
} from "@openaide/app-shell-contracts";
import type { DesktopNotificationSettings } from "../../shells/webTaskNotifications";

export function GeneralSettingsTab({
  developerSettingsUnlocked = false,
  desktopNotifications,
  onSetAcpTrace,
  onSetComposerSubmitShortcut,
  onSetDesktopNotifications,
  preferences,
  runtimeSettings,
}: {
  developerSettingsUnlocked?: boolean;
  desktopNotifications?: DesktopNotificationSettings;
  onSetAcpTrace: (enabled: boolean) => void;
  onSetComposerSubmitShortcut: (shortcut: ComposerSubmitShortcut) => void;
  onSetDesktopNotifications?: (enabled: boolean) => void | Promise<void>;
  preferences: AppPreferencesRecord;
  runtimeSettings?: RuntimeSettingsResult;
}) {
  const [query, setQuery] = useState("");
  const enterSends = preferences.composer_submit_shortcut === "enter";
  const newLineShortcut = enterSends ? "Ctrl/Cmd+Enter" : "Enter";
  const developerSettings = runtimeSettings?.developer;
  const groups: GeneralSettingsGroup[] = [
    {
      id: "composer",
      label: "Composer",
      rows: [
        {
          id: "enter-sends-message",
          label: "Enter sends message",
          detail: "Send from the composer with Enter.",
          searchText: `composer enter sends message send ${enterSends ? "on" : "off"}`,
          value: (
            <label className="settings-switch" aria-label="Enter sends message">
              <input
                aria-label="Enter sends message"
                checked={enterSends}
                onChange={(event) => onSetComposerSubmitShortcut(event.currentTarget.checked ? "enter" : "mod_enter")}
                type="checkbox"
              />
              <span className="settings-switch-track" aria-hidden="true" />
            </label>
          ),
        },
        {
          id: "new-line-shortcut",
          label: "New line shortcut",
          searchText: `composer new line shortcut ${newLineShortcut}`,
          value: <span className="settings-row-value">{newLineShortcut}</span>,
        },
      ],
    },
  ];

  if (desktopNotifications && onSetDesktopNotifications) {
    groups.push({
      id: "notifications",
      label: "Notifications",
      rows: [{
        id: "desktop-notifications",
        label: "Desktop notifications",
        detail: desktopNotificationDetail(desktopNotifications.status),
        searchText: `desktop notifications operating system ${desktopNotifications.status}`,
        value: (
          <label className="settings-switch" aria-label="Desktop notifications">
            <input
              aria-label="Desktop notifications"
              checked={desktopNotifications.status === "enabled" || desktopNotifications.status === "blocked"}
              disabled={desktopNotifications.status === "unsupported"}
              onChange={(event) => { void onSetDesktopNotifications(event.currentTarget.checked); }}
              type="checkbox"
            />
            <span className="settings-switch-track" aria-hidden="true" />
          </label>
        ),
      }],
    });
  }

  if (developerSettings && developerSettingsUnlocked) {
    groups.push({
      id: "developer",
      label: "Developer",
      rows: [
        {
          id: "acp-logs",
          label: "ACP logs",
          detail: "Write ACP trace files for local debugging.",
          searchText: `developer acp logs trace ${developerSettings.acp_trace.enabled ? "on" : "off"}`,
          value: (
            <label className="settings-switch" aria-label="ACP logs">
              <input
                aria-label="ACP logs"
                checked={developerSettings.acp_trace.enabled}
                onChange={(event) => onSetAcpTrace(event.currentTarget.checked)}
                type="checkbox"
              />
              <span className="settings-switch-track" aria-hidden="true" />
            </label>
          ),
        },
        {
          id: "trace-directory",
          label: "Trace directory",
          searchText: `developer trace directory ${developerSettings.acp_trace.directory}`,
          value: (
            <code className="settings-row-value" title={developerSettings.acp_trace.directory}>
              {compactPathForSettings(developerSettings.acp_trace.directory)}
            </code>
          ),
        },
      ],
    });
  }

  const normalizedQuery = query.trim().toLowerCase();
  const visibleGroups = normalizedQuery
    ? groups
        .map((group) => ({
          ...group,
          rows: group.rows.filter((row) => `${group.label} ${row.label} ${row.detail ?? ""} ${row.searchText}`.toLowerCase().includes(normalizedQuery)),
        }))
        .filter((group) => group.rows.length)
    : groups;

  return (
    <div className="settings-panel">
      <div className="settings-panel-title">
        <SlidersHorizontal size={15} />
        <span>
          <strong>General</strong>
          <small>{developerSettingsUnlocked ? "Composer behavior and developer diagnostics." : "Composer behavior."}</small>
        </span>
      </div>
      <label className="settings-filter">
        <Search size={13} />
        <input
          aria-label="Search settings"
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search settings"
          type="search"
        />
      </label>
      <div className="settings-common-list">
        {visibleGroups.length ? (
          visibleGroups.map((group) => (
            <section className="settings-section" aria-label={group.label} key={group.id}>
              <div className="settings-section-title">
                <strong>{group.label}</strong>
              </div>
              {group.rows.map((row) => (
                <div className="settings-row" key={row.id}>
                  <span className="settings-row-copy">
                    <strong>{row.label}</strong>
                    {row.detail ? <small>{row.detail}</small> : null}
                  </span>
                  {row.value}
                </div>
              ))}
            </section>
          ))
        ) : (
          <div className="settings-empty">
            <strong>No settings found</strong>
            <span>Try a different search.</span>
          </div>
        )}
      </div>
    </div>
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

type GeneralSettingsGroup = {
  id: string;
  label: string;
  rows: GeneralSettingsRow[];
};

type GeneralSettingsRow = {
  id: string;
  label: string;
  detail?: string;
  searchText: string;
  value: ReactNode;
};

export function compactPathForSettings(path: string, visibleSegments = 3): string {
  if (!path) return path;
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= visibleSegments) return path;
  const separator = path.includes("\\") ? "\\" : "/";
  return `...${separator}${parts.slice(-visibleSegments).join(separator)}`;
}
