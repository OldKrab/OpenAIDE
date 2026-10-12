/** Settings sections, named by what the user came to do (ADR 0065). */
export const settingsTabIds = ["general", "agents", "mcp", "skills", "worktrees", "remote", "troubleshooting"] as const;
export type SettingsTabId = (typeof settingsTabIds)[number];

/** Section ids from before ADR 0065, kept so saved links and shell commands still land. */
const legacySettingsTabIds: Record<string, SettingsTabId> = {
  common: "general",
  desktop: "general",
  connection: "remote",
  devices: "remote",
  data: "troubleshooting",
};

/** Reads a section id from a route, dataset, or message; unknown values select no section. */
export function settingsTabFromValue(value: unknown): SettingsTabId | undefined {
  if (typeof value !== "string") return undefined;
  if ((settingsTabIds as readonly string[]).includes(value)) return value as SettingsTabId;
  return Object.hasOwn(legacySettingsTabIds, value) ? legacySettingsTabIds[value] : undefined;
}
export type SettingsScope = "global" | "workspace";
export type ComposerSubmitShortcut = "mod_enter" | "enter";

export type AppPreferencesRecord = {
  composer_submit_shortcut: ComposerSubmitShortcut;
};
