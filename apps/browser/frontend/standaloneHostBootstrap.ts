import type { WebviewBootstrap } from "@openaide/app-shell-contracts";

export type StandaloneBootstrapInput = {
  hasDatasetSurface: boolean;
  hasVsCodeApi: boolean;
  pathname: string;
};

export function standaloneBootstrapFrom(input: StandaloneBootstrapInput): WebviewBootstrap | undefined {
  if (input.hasVsCodeApi || input.hasDatasetSurface) return undefined;
  const surface = surfaceFromPath(input.pathname);
  return {
    surface,
    taskId: surface === "task" && !isNewTaskPath(input.pathname) ? "demo_task" : undefined,
    preferences: { composer_submit_shortcut: "enter" },
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
