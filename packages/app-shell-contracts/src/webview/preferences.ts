export type SettingsTabId = "agents" | "mcp" | "skills" | "common" | "worktrees";
export type SettingsScope = "global" | "workspace";
export type ComposerSubmitShortcut = "mod_enter" | "enter";
export type AppTheme = "system" | "light" | "dark";

export type AppPreferencesRecord = {
  composer_submit_shortcut: ComposerSubmitShortcut;
  /** Optional only for compatibility with App Shell bootstraps from before theme preferences. */
  theme?: AppTheme;
};
