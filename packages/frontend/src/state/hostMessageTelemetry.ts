import type { PostHostMessage } from "./postHostMessage";

export function sendWebviewTelemetry(postHostMessage: PostHostMessage, event: string, fields: Record<string, unknown>) {
  postHostMessage({
    type: "webview.telemetry",
    payload: {
      event,
      ...fields,
    },
  });
}
