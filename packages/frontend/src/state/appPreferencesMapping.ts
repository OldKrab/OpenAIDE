import type {
  AppPreferencesResult,
  AppTheme as ProtocolAppTheme,
  ComposerSubmitShortcut as ProtocolComposerSubmitShortcut,
} from "@openaide/app-server-client";
import type {
  AppPreferencesRecord,
  AppTheme,
  ComposerSubmitShortcut,
} from "@openaide/app-shell-contracts";

export function mapProtocolAppPreferences(result: AppPreferencesResult): AppPreferencesRecord {
  return {
    composer_submit_shortcut: mapProtocolComposerSubmitShortcut(result.preferences.composerSubmitShortcut),
    theme: mapProtocolAppTheme(result.preferences.theme),
  };
}

export function protocolComposerSubmitShortcut(shortcut: ComposerSubmitShortcut): ProtocolComposerSubmitShortcut {
  return shortcut === "enter" ? "enter" : "modEnter";
}

export function protocolAppTheme(theme: AppTheme): ProtocolAppTheme {
  return theme;
}

function mapProtocolComposerSubmitShortcut(shortcut: ProtocolComposerSubmitShortcut): ComposerSubmitShortcut {
  return shortcut === "enter" ? "enter" : "mod_enter";
}

function mapProtocolAppTheme(theme: ProtocolAppTheme): AppTheme {
  return theme;
}
