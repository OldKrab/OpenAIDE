import type { WebviewBootstrap, WebviewSurfaceKind } from "@openaide/app-shell-contracts";

export type SurfaceKind = WebviewSurfaceKind;
export type { WebviewBootstrap };

export const VSCODE_SHELL = {
  kind: "vscodeExtension",
  navigationMode: "currentProject",
} as const satisfies WebviewBootstrap["shell"];

export type WebviewHost = {
  openNewTask: (projectId?: string) => void;
  openSettings: () => void;
  openTask: (taskId: string, title?: string) => void;
};
