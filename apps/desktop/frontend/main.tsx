import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createReliableLocalHttpBackendConnection } from "@openaide/app-server-client";
import { startFrontend } from "../../../packages/frontend/src/startFrontend";
import { createDesktopShell } from "./desktopShell";
import {
  runDesktopBootstrap,
  type LocalHttpConnection,
} from "./desktopBootstrap";

const startupStartedAt = performance.now();
const startupElapsedTimer = window.setInterval(updateStartupElapsed, 1_000);

async function start() {
  const bootstrap = await runDesktopBootstrap({
    invoke,
    listen,
    onProgress: ({ message }) => updateStartupStatus(message),
    onContext: (context) => {
      const cancel = document.querySelector<HTMLButtonElement>(".desktop-startup-cancel");
      if (cancel) {
        cancel.hidden = context.environment.kind !== "wsl" || !context.canRecover;
        cancel.onclick = () => { void invoke("recover_desktop_runtime_environment"); };
      }
    },
  });
  window.clearInterval(startupElapsedTimer);
  const session = createReliableLocalHttpBackendConnection({
    ...bootstrap.connection,
    connectionId: bootstrap.clientInstanceId,
    subscribeToReplacement(replace) {
      let disposed = false;
      let stop: (() => void) | undefined;
      void listen<LocalHttpConnection>("desktop-app-server-replaced", ({ payload }) => {
        replace(payload);
      }).then((unsubscribe) => {
        if (disposed) unsubscribe();
        else stop = unsubscribe;
      });
      return () => {
        disposed = true;
        stop?.();
      };
    },
  });
  const shell = createDesktopShell(bootstrap, session);
  startFrontend(shell);
  void invoke("desktop_update_mark_interactive")
    .then(() => invoke("desktop_auto_check_for_update"))
    .catch(() => undefined);
}

function updateStartupStatus(message: string) {
  const label = document.querySelector<HTMLElement>(".desktop-startup-label");
  if (label) label.textContent = message;
}

function updateStartupElapsed() {
  const elapsed = document.querySelector<HTMLElement>(".desktop-startup-elapsed");
  if (elapsed) elapsed.textContent = `${Math.floor((performance.now() - startupStartedAt) / 1_000)}s`;
}

void start().catch((error) => {
  window.clearInterval(startupElapsedTimer);
  const root = document.getElementById("root");
  if (root) {
    const message = error instanceof Error
      ? `OpenAIDE could not start: ${error.message}`
      : "OpenAIDE could not start.";
    const label = root.querySelector<HTMLElement>(".desktop-startup-label");
    if (label) label.textContent = message;
    const retry = root.querySelector<HTMLButtonElement>(".desktop-startup-retry");
    if (retry) {
      retry.hidden = false;
      retry.onclick = () => window.location.reload();
    }
  }
});
