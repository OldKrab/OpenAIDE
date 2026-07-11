import { Component, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import type { WebviewSurfaceKind } from "@openaide/app-shell-contracts";
import { App } from "./components/App";
import { postHostMessage } from "./services/hostBridge";
import "@fontsource-variable/inter";
import "./styles/tokens.css";
import "./styles/app.css";

type ErrorBoundaryState = {
  error?: Error;
};

class WebviewErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, _info: ErrorInfo) {
    sendWebviewDiagnostics("render_error", {
      error_name: error.name,
      error_message: error.message,
    });
  }

  render() {
    if (this.state.error) {
      return (
        <main className="app-shell editor-shell">
          <section className="task-surface task-loading" aria-label="OpenAIDE render error">
            <p>OpenAIDE could not render this view.</p>
          </section>
        </main>
      );
    }
    return this.props.children;
  }
}

window.addEventListener("error", (event) => {
  sendWebviewDiagnostics("window_error", {
    error_name: event.error instanceof Error ? event.error.name : "Error",
    error_message: event.error instanceof Error ? event.error.message : event.message,
  });
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  sendWebviewDiagnostics("unhandled_rejection", {
    error_name: reason instanceof Error ? reason.name : "Error",
    error_message: reason instanceof Error ? reason.message : String(reason),
  });
});

createRoot(document.getElementById("root")!).render(
  <WebviewErrorBoundary>
    <App />
  </WebviewErrorBoundary>,
);

function sendWebviewDiagnostics(event: string, fields: Record<string, unknown>) {
  postHostMessage({
    type: "webview.telemetry",
    payload: {
      event,
      surface: webviewSurfaceForTelemetry(document.body.dataset.surface),
      task_id: document.body.dataset.taskId,
      ...fields,
    },
  });
}

function webviewSurfaceForTelemetry(surface: string | undefined): WebviewSurfaceKind | "invalid" | undefined {
  if (surface === "navigation" || surface === "settings" || surface === "task") return surface;
  return surface === undefined ? undefined : "invalid";
}
