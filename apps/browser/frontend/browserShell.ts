import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import { createVsCodeShell } from "../../vscode-extension/frontend/vsCodeShell";
import { createWebAppShell } from "../../web/frontend/webAppShell";
import { createStandaloneShell } from "./standaloneShell";

/** Selects the concrete browser App Shell before mounting the shared Frontend. */
export function createBrowserShell(): FrontendShell {
  if (document.body.dataset.shell === "web") return createWebAppShell();
  if (window.acquireVsCodeApi || document.body.dataset.surface) return createVsCodeShell();
  return createStandaloneShell();
}
