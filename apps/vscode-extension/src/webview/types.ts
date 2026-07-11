import type { WebviewBootstrap, WebviewSurfaceKind } from "@openaide/app-shell-contracts";

export type SurfaceKind = WebviewSurfaceKind;
export type { WebviewBootstrap };

export type WebviewHost = {
  openNewTask: (projectId?: string) => void;
  openSettings: () => void;
  openTask: (taskId: string, title?: string) => void;
};
