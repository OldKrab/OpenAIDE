import type { FrontendShell } from "../services/frontendShell";
import { createWebAppShell } from "../../../../apps/web/frontend/webAppShell";
import { createVsCodeShell } from "../../../../apps/vscode-extension/frontend/vsCodeShell";
import { createStandaloneShell } from "./standaloneShell";

/** Browser entry-point composition: select one concrete adapter before mounting shared UI. */
export function createBrowserShell(): FrontendShell {
  if (document.body.dataset.shell === "web") return createWebAppShell();
  if (window.acquireVsCodeApi || document.body.dataset.surface) return createVsCodeShell();
  return createStandaloneShell();
}
