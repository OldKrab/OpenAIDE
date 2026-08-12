export const MERMAID_RENDERER_VERSION = "11.16.1";
export const MERMAID_SOURCE_LIMIT = 20_000;
export const MERMAID_EDGE_LIMIT = 200;
export const MERMAID_OUTPUT_LIMIT_BYTES = 2 * 1024 * 1024;
export const MERMAID_RENDER_TIMEOUT_MS = 3_000;

export type MermaidTheme = {
  background: string;
  border: string;
  highContrast: boolean;
  muted: string;
  panel: string;
  primary: string;
  text: string;
  fontFamily: string;
};

export type MermaidRenderRequest = {
  type: "openaide.mermaid.render";
  requestId: string;
  source: string;
  theme: MermaidTheme;
};

export type MermaidRendererReady = {
  type: "openaide.mermaid.ready";
  version: string;
};

export type MermaidRenderResponse = {
  type: "openaide.mermaid.result";
  requestId: string;
  result:
    | { kind: "success"; svg: string; title?: string; description?: string; outputBytes: number }
    | { kind: "invalid" }
    | { kind: "too_large" }
    | { kind: "failed" };
};

export function isMermaidRenderRequest(value: unknown): value is MermaidRenderRequest {
  if (!isRecord(value) || value.type !== "openaide.mermaid.render") return false;
  if (typeof value.requestId !== "string" || !/^[a-zA-Z0-9-]{1,80}$/.test(value.requestId)) return false;
  if (typeof value.source !== "string" || value.source.length > MERMAID_SOURCE_LIMIT) return false;
  return isMermaidTheme(value.theme);
}

export function isMermaidRendererReady(value: unknown): value is MermaidRendererReady {
  return isRecord(value)
    && value.type === "openaide.mermaid.ready"
    && value.version === MERMAID_RENDERER_VERSION;
}

export function isMermaidRenderResponse(value: unknown): value is MermaidRenderResponse {
  if (!isRecord(value) || value.type !== "openaide.mermaid.result" || typeof value.requestId !== "string") return false;
  const result = value.result;
  if (!isRecord(result) || !["success", "invalid", "too_large", "failed"].includes(String(result.kind))) return false;
  if (result.kind !== "success") return true;
  return typeof result.svg === "string"
    && result.svg.length <= MERMAID_OUTPUT_LIMIT_BYTES
    && typeof result.outputBytes === "number"
    && (result.title === undefined || typeof result.title === "string")
    && (result.description === undefined || typeof result.description === "string");
}

function isMermaidTheme(value: unknown): value is MermaidTheme {
  if (!isRecord(value) || typeof value.highContrast !== "boolean") return false;
  return ["background", "border", "muted", "panel", "primary", "text", "fontFamily"]
    .every((key) => typeof value[key] === "string" && value[key].length <= 200);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
