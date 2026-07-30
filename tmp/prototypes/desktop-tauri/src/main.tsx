import { invoke } from "@tauri-apps/api/core";
import { startFrontend } from "../../../../packages/frontend/src/startFrontend";
import { createDesktopPrototypeShell } from "./prototypeDesktopShell";

type LocalHttpConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
};

async function main() {
  const connection = await invoke<LocalHttpConnection>("app_server_connection");
  startFrontend(createDesktopPrototypeShell(connection));
}

void main().catch((error) => {
  console.error("desktop prototype startup failed", error);
  const root = document.getElementById("root");
  if (root) {
    root.innerHTML = `
      <main style="font: 14px system-ui; padding: 32px">
        <h1>OpenAIDE Desktop Prototype could not start</h1>
        <p>Check the terminal for App Server handoff details.</p>
      </main>
    `;
  }
});
