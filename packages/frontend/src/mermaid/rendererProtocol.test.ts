import { describe, expect, it } from "vitest";
import {
  MERMAID_RENDERER_VERSION,
  MERMAID_SOURCE_LIMIT,
  isMermaidRenderRequest,
  isMermaidRenderResponse,
  isMermaidRendererReady,
} from "./rendererProtocol";

const theme = {
  background: "rgb(10, 10, 10)",
  border: "rgb(80, 80, 80)",
  highContrast: false,
  muted: "rgb(140, 140, 140)",
  panel: "rgb(20, 20, 20)",
  primary: "rgb(80, 140, 240)",
  text: "rgb(230, 230, 230)",
  fontFamily: "sans-serif",
};

describe("Mermaid renderer protocol", () => {
  it("accepts only bounded typed requests", () => {
    expect(isMermaidRenderRequest({
      type: "openaide.mermaid.render",
      requestId: "request-1",
      source: "flowchart LR\nA --> B",
      theme,
    })).toBe(true);
    expect(isMermaidRenderRequest({
      type: "openaide.mermaid.render",
      requestId: "request-2",
      source: "x".repeat(MERMAID_SOURCE_LIMIT + 1),
      theme,
    })).toBe(false);
  });

  it("rejects renderer messages with the wrong version or malformed success output", () => {
    expect(isMermaidRendererReady({ type: "openaide.mermaid.ready", version: MERMAID_RENDERER_VERSION })).toBe(true);
    expect(isMermaidRendererReady({ type: "openaide.mermaid.ready", version: "old" })).toBe(false);
    expect(isMermaidRenderResponse({
      type: "openaide.mermaid.result",
      requestId: "request-1",
      result: { kind: "success", svg: 42, outputBytes: 2 },
    })).toBe(false);
  });
});
