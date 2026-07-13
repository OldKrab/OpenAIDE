import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import { standaloneBootstrapFrom } from "./standaloneHostBootstrap";
import { handleStandaloneHostMessage } from "./standaloneHostRouter";

export function standaloneBootstrap() {
  if (typeof window === "undefined" || typeof document === "undefined") return undefined;
  return standaloneBootstrapFrom({
    hasDatasetSurface: Boolean(document.body.dataset.surface),
    hasVsCodeApi: Boolean(window.acquireVsCodeApi),
    pathname: window.location.pathname,
  });
}

export function createStandaloneHost() {
  if (
    typeof window === "undefined"
    || typeof document === "undefined"
    || window.acquireVsCodeApi
    || document.body.dataset.surface
  ) return undefined;
  return {
    postMessage(message: unknown) {
      handleStandaloneHostMessage(message, {
        navigate(path) {
          window.history.pushState(null, "", path);
          window.location.reload();
        },
        post,
      });
    },
  };
}

function post(message: HostToWebviewMessage) {
  window.setTimeout(() => window.dispatchEvent(new MessageEvent("message", { data: message })), 0);
}
