import type { WebviewBootstrap as ValidWebviewBootstrap, WebviewSurfaceKind } from "@openaide/app-shell-contracts";

export type WebviewSurface = WebviewSurfaceKind | "invalid";

export type WebviewBootstrap =
  | (ValidWebviewBootstrap & { archived?: boolean })
  | { surface: "invalid"; taskId?: undefined; preferences?: undefined };
