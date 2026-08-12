import { postHostMessage } from "../services/hostBridge";
import { sendWebviewTelemetry } from "../state/hostMessageTelemetry";
import {
  MERMAID_OUTPUT_LIMIT_BYTES,
  MERMAID_RENDERER_VERSION,
  MERMAID_RENDER_TIMEOUT_MS,
  MERMAID_SOURCE_LIMIT,
  isMermaidRenderResponse,
  isMermaidRendererReady,
  type MermaidRenderRequest,
  type MermaidRenderResponse,
  type MermaidTheme,
} from "./rendererProtocol";

export type MermaidDiagramResult =
  | { kind: "success"; url: string; title?: string; description?: string }
  | { kind: "invalid" | "too_large" | "timeout" | "startup_timeout" | "unavailable" | "failed" };

type CachedDiagram = Extract<MermaidDiagramResult, { kind: "success" }> & {
  memoryBytes: number;
  outputBytes: number;
};

// The renderer is a large lazy-loaded bundle; slower external App Shells need a
// separate cold-start allowance before individual Diagram render deadlines apply.
const RENDERER_READY_TIMEOUT_MS = 30_000;
const CACHE_ENTRY_LIMIT = 32;
const CACHE_BYTE_LIMIT = 16 * 1024 * 1024;

class MermaidRenderCoordinator {
  private cache = new Map<string, CachedDiagram>();
  private cacheBytes = 0;
  private frame?: HTMLIFrameElement;
  private frameReady?: Promise<HTMLIFrameElement>;
  private queue: Promise<void> = Promise.resolve();
  private queueDepth = 0;

  render(source: string, theme: MermaidTheme, attempt = 1): Promise<MermaidDiagramResult> {
    const key = JSON.stringify([MERMAID_RENDERER_VERSION, theme, source]);
    const cached = this.cache.get(key);
    if (cached) {
      this.cache.delete(key);
      this.cache.set(key, cached);
      const operationId = globalThis.crypto.randomUUID();
      this.telemetry("diagram_render_start", {
        operation_id: operationId,
        attempt,
        cache_hit: true,
        queue_depth: this.queueDepth,
        source_length: source.length,
      });
      this.telemetry("diagram_render_terminal", {
        operation_id: operationId,
        attempt,
        cache_hit: true,
        duration_ms: 0,
        outcome: "success",
        output_bytes: cached.outputBytes,
        source_length: source.length,
      });
      return Promise.resolve({ kind: "success", url: cached.url, title: cached.title, description: cached.description });
    }
    this.queueDepth += 1;
    const job = this.queue.then(() => this.renderUncached(source, theme, key, attempt));
    this.queue = job.then(() => undefined, () => undefined);
    return job.finally(() => { this.queueDepth = Math.max(0, this.queueDepth - 1); });
  }

  private async renderUncached(source: string, theme: MermaidTheme, key: string, attempt: number) {
    const operationId = globalThis.crypto.randomUUID();
    const startedAt = performance.now();
    this.telemetry("diagram_render_start", {
      operation_id: operationId,
      attempt,
      cache_hit: false,
      queue_depth: Math.max(0, this.queueDepth - 1),
      source_length: source.length,
    });
    let result: MermaidDiagramResult;
    let outputBytes: number | undefined;
    if (source.length > MERMAID_SOURCE_LIMIT) {
      result = { kind: "too_large" };
    } else {
      try {
        const response = await this.requestRender(source, theme);
        if (response.result.kind === "success") {
          outputBytes = response.result.outputBytes;
          if (outputBytes > MERMAID_OUTPUT_LIMIT_BYTES || !response.result.svg.trimStart().startsWith("<svg")) {
            result = { kind: "failed" };
          } else {
            const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(response.result.svg)}`;
            result = {
              kind: "success",
              url,
              title: response.result.title,
              description: response.result.description,
            };
            this.remember(key, {
              ...result,
              memoryBytes: url.length * 2,
              outputBytes,
            });
          }
        } else {
          result = response.result;
        }
      } catch (error) {
        result = {
          kind: error instanceof RenderTimeoutError
            ? "timeout"
            : error instanceof RendererStartupTimeoutError
              ? "startup_timeout"
              : "unavailable",
        };
      }
    }
    this.telemetry("diagram_render_terminal", {
      operation_id: operationId,
      attempt,
      cache_hit: false,
      duration_ms: Math.round(performance.now() - startedAt),
      outcome: result.kind,
      output_bytes: outputBytes,
      source_length: source.length,
    });
    return result;
  }

  private async requestRender(source: string, theme: MermaidTheme): Promise<MermaidRenderResponse> {
    const frame = await this.ensureFrame();
    const requestId = globalThis.crypto.randomUUID();
    const request: MermaidRenderRequest = { type: "openaide.mermaid.render", requestId, source, theme };
    return new Promise((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        cleanup();
        this.resetFrame(frame);
        reject(new RenderTimeoutError());
      }, MERMAID_RENDER_TIMEOUT_MS);
      const onMessage = (event: MessageEvent) => {
        if (event.source !== frame.contentWindow || !isMermaidRenderResponse(event.data) || event.data.requestId !== requestId) return;
        cleanup();
        resolve(event.data);
      };
      const cleanup = () => {
        window.clearTimeout(timeout);
        window.removeEventListener("message", onMessage);
      };
      window.addEventListener("message", onMessage);
      frame.contentWindow?.postMessage(request, "*");
    });
  }

  private ensureFrame() {
    if (this.frame && this.frameReady) return this.frameReady;
    const frame = document.createElement("iframe");
    frame.className = "mermaid-renderer-frame";
    frame.setAttribute("aria-hidden", "true");
    frame.setAttribute("sandbox", "allow-scripts");
    this.frame = frame;
    this.frameReady = new Promise<HTMLIFrameElement>((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        cleanup();
        this.resetFrame(frame);
        reject(new RendererStartupTimeoutError());
      }, RENDERER_READY_TIMEOUT_MS);
      const onMessage = (event: MessageEvent) => {
        if (event.source !== frame.contentWindow || !isMermaidRendererReady(event.data)) return;
        cleanup();
        resolve(frame);
      };
      const onError = () => {
        cleanup();
        this.resetFrame(frame);
        reject(new Error("renderer unavailable"));
      };
      const cleanup = () => {
        window.clearTimeout(timeout);
        window.removeEventListener("message", onMessage);
        frame.removeEventListener("error", onError);
      };
      window.addEventListener("message", onMessage);
      frame.addEventListener("error", onError);
      document.body.append(frame);
      void configureRendererFrame(frame).catch(onError);
    });
    return this.frameReady;
  }

  private resetFrame(frame: HTMLIFrameElement) {
    if (this.frame !== frame) return;
    frame.remove();
    this.frame = undefined;
    this.frameReady = undefined;
  }

  private remember(key: string, diagram: CachedDiagram) {
    this.cache.set(key, diagram);
    this.cacheBytes += diagram.memoryBytes;
    while (this.cache.size > CACHE_ENTRY_LIMIT || this.cacheBytes > CACHE_BYTE_LIMIT) {
      const oldestKey = this.cache.keys().next().value as string | undefined;
      if (!oldestKey) break;
      const oldest = this.cache.get(oldestKey);
      this.cache.delete(oldestKey);
      this.cacheBytes -= oldest?.memoryBytes ?? 0;
    }
  }

  private telemetry(event: string, fields: Record<string, unknown>) {
    sendWebviewTelemetry(postHostMessage, event, fields);
  }
}

class RenderTimeoutError extends Error {}
class RendererStartupTimeoutError extends Error {}

function configuredRendererUri() {
  const configured = document.querySelector<HTMLMetaElement>('meta[name="openaide-mermaid-renderer"]')?.content;
  return configured || undefined;
}

/**
 * Web authentication cookies are unavailable inside the opaque sandbox. The
 * parent therefore fetches the packaged renderer and gives the child a
 * self-contained document, while VS Code uses its admitted resource URI.
 */
async function configureRendererFrame(frame: HTMLIFrameElement) {
  const configured = configuredRendererUri();
  if (configured) {
    frame.src = configured;
    return;
  }
  const documentUri = new URL("/mermaid-renderer.html", window.location.href);
  const scriptUri = new URL("/mermaid-renderer.js", window.location.href);
  const [documentResponse, scriptResponse] = await Promise.all([fetch(documentUri), fetch(scriptUri)]);
  if (!documentResponse.ok || !scriptResponse.ok) throw new Error("renderer unavailable");
  const rendererDocument = await documentResponse.text();
  if (!rendererDocument.includes('src="./mermaid-renderer.js"')) throw new Error("renderer unavailable");
  const rendererScript = await scriptResponse.text();
  frame.srcdoc = buildSelfContainedRendererDocument(rendererDocument, rendererScript);
}

export function buildSelfContainedRendererDocument(rendererDocument: string, rendererScript: string) {
  const bytes = new TextEncoder().encode(rendererScript);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 32_768));
  }
  const payload = globalThis.btoa(binary);
  const embeddedRenderer = `<script nonce="openaide-mermaid-renderer" type="application/octet-stream" id="openaide-mermaid-payload">${payload}</script>
    <script nonce="openaide-mermaid-renderer">
      const payload = document.getElementById("openaide-mermaid-payload").textContent || "";
      const binary = atob(payload);
      const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
      const script = document.createElement("script");
      script.nonce = "openaide-mermaid-renderer";
      script.textContent = new TextDecoder().decode(bytes);
      document.head.append(script);
    </script>`;
  return rendererDocument.replace(
    '<script nonce="openaide-mermaid-renderer" src="./mermaid-renderer.js"></script>',
    embeddedRenderer,
  );
}

let coordinator: MermaidRenderCoordinator | undefined;

export function renderMermaidDiagram(source: string, theme: MermaidTheme, attempt = 1) {
  coordinator ??= new MermaidRenderCoordinator();
  return coordinator.render(source, theme, attempt);
}
