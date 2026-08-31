type PreparedProjectFolder = {
  label: string;
  path: string;
  warning?: { key: string; message: string };
};

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

/** Keeps native folder acquisition in Desktop while the shell translates paths for its active backend OS. */
export function createDesktopProjectPicker({
  confirm,
  invoke,
  openDirectory,
}: {
  confirm(message: string): boolean;
  invoke: Invoke;
  openDirectory(): Promise<string | string[] | null>;
}) {
  const acknowledgedWarnings = new Set<string>();
  return async () => {
    const selected = await openDirectory();
    if (typeof selected !== "string") return undefined;
    const prepared = await invoke("desktop_prepare_project_folder", { path: selected }) as PreparedProjectFolder;
    if (prepared.warning && !acknowledgedWarnings.has(prepared.warning.key)) {
      const accepted = confirm(`${prepared.warning.message}\n\nContinue with this folder?`);
      if (!accepted) return undefined;
      if (confirm("Don't show this cross-environment warning again?")) {
        acknowledgedWarnings.add(prepared.warning.key);
        await invoke("desktop_dismiss_path_warning", { key: prepared.warning.key });
      }
    }
    return { label: prepared.label, path: prepared.path };
  };
}
