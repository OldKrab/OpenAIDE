import type { AppPreferencesResult, ComposerSubmitShortcut as ProtocolComposerSubmitShortcut } from "@openaide/app-server-client";
import type { AppPreferencesRecord, ComposerSubmitShortcut } from "@openaide/app-shell-contracts";

export function mapProtocolAppPreferences(result: AppPreferencesResult): AppPreferencesRecord {
  return {
    composer_submit_shortcut: mapProtocolComposerSubmitShortcut(result.preferences.composerSubmitShortcut),
  };
}

export function protocolComposerSubmitShortcut(shortcut: ComposerSubmitShortcut): ProtocolComposerSubmitShortcut {
  return shortcut === "enter" ? "enter" : "modEnter";
}

function mapProtocolComposerSubmitShortcut(shortcut: ProtocolComposerSubmitShortcut): ComposerSubmitShortcut {
  return shortcut === "enter" ? "enter" : "mod_enter";
}
