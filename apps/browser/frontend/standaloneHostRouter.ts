import type { HostToWebviewMessage, WebviewToHostMessage } from "@openaide/app-shell-contracts";
import { createStandaloneHostData } from "./standaloneHostData";

export type StandaloneHostOutput = {
  navigate(path: string): void;
  post(message: HostToWebviewMessage): void;
};

export function handleStandaloneHostMessage(message: unknown, output: StandaloneHostOutput) {
  if (!isWebviewMessage(message)) return;
  routeStandaloneHostMessage(message, output);
}

function routeStandaloneHostMessage(message: WebviewToHostMessage, output: StandaloneHostOutput) {
  const data = createStandaloneHostData();
  switch (message.type) {
    case "secret.transaction.apply":
    case "secret.transaction.commit":
    case "secret.transaction.rollback":
      output.post({
        type: "secret.transaction.result",
        payload: {
          requestId: message.payload.requestId,
          transactionId: message.payload.transactionId,
          ok: false,
          error: "Secure storage is unavailable in the standalone preview.",
        },
      });
      return;
    case "workspace.roots":
      output.post({ type: "workspace.roots.result", payload: { roots: data.workspaceRoots() } });
      return;
    case "developer.settings.unlock":
      output.post({
        type: "runtime.settings.result",
        payload: { developer: { acp_trace: { enabled: true, directory: data.traceDirectory() } } },
      });
      return;
    case "surface.openTask":
      output.navigate("/task");
      return;
    case "surface.openNewTask":
      output.navigate(message.payload?.project_id
        ? `/new-task?projectId=${encodeURIComponent(message.payload.project_id)}`
        : "/new-task");
      return;
    case "surface.openSettings":
      output.navigate(standaloneSettingsPath(message.payload));
      return;
    default:
      return;
  }
}

function standaloneSettingsPath(payload: Extract<WebviewToHostMessage, { type: "surface.openSettings" }>["payload"]) {
  const search = new URLSearchParams();
  if (payload?.settings_tab) search.set("tab", payload.settings_tab);
  if (payload?.settings_intent?.kind === "openSupportExport") {
    search.set("intent", payload.settings_intent.kind);
    search.set("intentRequestId", payload.settings_intent.requestId);
  }
  const query = search.toString();
  return query ? `/settings?${query}` : "/settings";
}

function isWebviewMessage(message: unknown): message is WebviewToHostMessage {
  return typeof message === "object"
    && message !== null
    && typeof (message as { type?: unknown }).type === "string";
}
