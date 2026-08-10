import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createReliableLocalHttpBackendConnection } from "@openaide/app-server-client";
import { startFrontend } from "../../../packages/frontend/src/startFrontend";
import { createDesktopValidationShell } from "./desktopShell";

type LocalHttpConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
};

type DesktopBootstrap = {
  clientInstanceId: string;
  connection: LocalHttpConnection;
  platform: "linux" | "macos" | "windows";
};

async function start() {
  const bootstrap = await invoke<DesktopBootstrap>("desktop_bootstrap");
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
  startFrontend(createDesktopValidationShell(bootstrap, session));
}

void start().catch((error) => {
  const root = document.getElementById("root");
  if (root) {
    root.textContent = error instanceof Error
      ? `OpenAIDE could not start: ${error.message}`
      : "OpenAIDE could not start.";
  }
});
