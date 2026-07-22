import type { WebviewBootstrap, WebviewSurfaceKind } from "@openaide/app-shell-contracts";

export type SurfaceKind = WebviewSurfaceKind;
export type { WebviewBootstrap };

export const VSCODE_SHELL = {
  kind: "vscodeExtension",
  navigationMode: "currentProject",
} as const satisfies WebviewBootstrap["shell"];

export type WebviewHost = {
  openNewTask: (projectId?: string) => void;
  openNativeSession: (agentId: string, nativeSessionId: string, projectId?: string) => void;
  openSettings: (agentId?: string, returnToNewTask?: boolean, projectId?: string) => void;
  openTask: (taskId: string, title?: string) => void;
};

/** Exposes shell-local editor focus without promoting it into App Server product state. */
export type TaskFocusSource = {
  currentFocusedTaskId: () => string | undefined;
  onDidChangeFocusedTask: (
    listener: (taskId: string | undefined) => void,
  ) => { dispose: () => void };
};
