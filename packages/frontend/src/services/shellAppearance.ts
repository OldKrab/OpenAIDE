import type { AppThemePreference, FrontendShellAppearance } from "./frontendShell";

type ThemeMediaQuery = {
  matches: boolean;
  addEventListener(type: "change", listener: () => void): void;
};

/** Creates shell-local theme persistence while exposing only the shared appearance contract. */
export function createShellAppearance({
  body,
  storage,
  storageKey,
  systemTheme,
  subscribeStoredTheme,
  onResolvedThemeChange,
}: {
  body: Pick<HTMLElement, "dataset">;
  onResolvedThemeChange?: (theme: "light" | "dark") => void;
  storage: Pick<Storage, "getItem" | "setItem">;
  storageKey: string;
  systemTheme?: ThemeMediaQuery;
  subscribeStoredTheme?: (listener: (value: string | null) => void) => void;
}): FrontendShellAppearance {
  let preference = readPreference(storage, storageKey);
  const apply = () => {
    const resolvedTheme = preference === "system"
      ? (systemTheme?.matches ? "dark" : "light")
      : preference;
    body.dataset.theme = resolvedTheme;
    onResolvedThemeChange?.(resolvedTheme);
  };
  systemTheme?.addEventListener("change", apply);
  subscribeStoredTheme?.((value) => {
    preference = themePreference(value);
    apply();
  });
  apply();
  return {
    theme: () => preference,
    setTheme(theme) {
      preference = theme;
      try {
        storage.setItem(storageKey, theme);
      } catch {
        // The current window still honors the choice when profile storage is unavailable.
      }
      apply();
    },
  };
}

function readPreference(storage: Pick<Storage, "getItem">, storageKey: string) {
  try {
    return themePreference(storage.getItem(storageKey));
  } catch {
    return "system";
  }
}

function themePreference(value: string | null): AppThemePreference {
  return value === "light" || value === "dark" ? value : "system";
}
