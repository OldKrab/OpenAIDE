export type SettingsTabId = "agents" | "mcp" | "skills" | "common";
export type SettingsScope = "global" | "workspace";
export type ComposerSubmitShortcut = "mod_enter" | "enter";

export type AppPreferencesRecord = {
  composer_submit_shortcut: ComposerSubmitShortcut;
};
