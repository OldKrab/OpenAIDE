import type { FileViewerContent } from "../../../packages/frontend/src/services/frontendShell";
import { readFileViewerContentOverHttp } from "../../../packages/frontend/src/services/fileViewerContentHttp";
import { localHttpRouteUrl, type DesktopBootstrap, type LocalHttpConnection } from "./desktopBootstrap";

/** Served by the native shell (`desktop_renderer_protocol.rs`) under the renderer's own CSP. */
const RENDERER_SCHEME = "openaide-renderer";

/**
 * Reads preview bytes from the App Server currently in use. The endpoint is read per request
 * because Desktop replaces it when the App Server restarts or the paired computer changes.
 */
export function createDesktopFileViewerContent(
  connection: () => LocalHttpConnection,
  clientInstanceId: string,
): FileViewerContent {
  return {
    read({ handle, maxBytes, operationId }, signal) {
      const { endpointUrl, authToken } = connection();
      const url = localHttpRouteUrl(endpointUrl, "download");
      if (!url) return Promise.resolve({ kind: "unavailable" });
      url.search = new URLSearchParams({
        clientInstanceId,
        fileViewerHandle: handle,
        operationId,
        preview: "1",
      }).toString();
      return readFileViewerContentOverHttp(url.toString(), { headers: { Authorization: `Bearer ${authToken}` } }, maxBytes, signal);
    },
  };
}

/** Webviews expose a custom scheme as `scheme://localhost` except on Windows. */
export function desktopPdfRendererDocumentUrl(platform: DesktopBootstrap["platform"]) {
  return platform === "windows"
    ? `http://${RENDERER_SCHEME}.localhost/pdf.html`
    : `${RENDERER_SCHEME}://localhost/pdf.html`;
}
