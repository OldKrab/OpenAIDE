import type { WebviewBootstrap } from "@openaide/app-shell-contracts";

export type StandaloneBootstrapInput = {
  hasDatasetSurface: boolean;
  hasVsCodeApi: boolean;
  pathname: string;
  search?: string;
};

export function standaloneBootstrapFrom(input: StandaloneBootstrapInput): WebviewBootstrap | undefined {
  if (input.hasVsCodeApi || input.hasDatasetSurface) return undefined;
  const surface = surfaceFromPath(input.pathname);
  const settings = surface === "settings" ? standaloneSettings(input.search ?? "") : {};
  return {
    surface,
    shell: { kind: "web", navigationMode: "project" },
    taskId: surface === "task" && !isNewTaskPath(input.pathname) ? "demo_task" : undefined,
    ...settings,
    preferences: { composer_submit_shortcut: "enter" },
  };
}

function standaloneSettings(searchValue: string) {
  const search = new URLSearchParams(searchValue);
  const tab = search.get("tab");
  const requestId = search.get("intentRequestId");
  return {
    settingsTab: tab === "agents" || tab === "mcp" || tab === "skills" || tab === "common"
      || tab === "desktop" || tab === "data" || tab === "worktrees"
      ? tab
      : undefined,
    settingsIntent: search.get("intent") === "openSupportExport" && requestId
      ? { kind: "openSupportExport" as const, requestId }
      : undefined,
  };
}

function surfaceFromPath(pathname: string): WebviewBootstrap["surface"] {
  if (pathname.includes("navigation")) return "navigation";
  if (pathname.includes("settings")) return "settings";
  return "task";
}

function isNewTaskPath(pathname: string) {
  return pathname.includes("new-task");
}
