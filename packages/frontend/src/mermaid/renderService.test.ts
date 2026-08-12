// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../services/hostBridge", () => ({ postHostMessage: vi.fn() }));
vi.mock("../state/hostMessageTelemetry", () => ({ sendWebviewTelemetry: vi.fn() }));

import { buildSelfContainedRendererDocument, renderMermaidDiagram } from "./renderService";

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

describe("Mermaid render service", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it("encodes the renderer bundle instead of placing executable text in HTML", () => {
    const document = buildSelfContainedRendererDocument(
      '<html><body><script nonce="openaide-mermaid-renderer" src="./mermaid-renderer.js"></script></body></html>',
      'const closingTag = "</script>";',
    );

    expect(document).not.toContain('const closingTag = "</script>";');
    expect(document).toContain(btoa('const closingTag = "</script>";'));
    expect(document).toContain('id="openaide-mermaid-payload"');
    expect(document).toContain('script.nonce = "openaide-mermaid-renderer"');
  });

  it("classifies a renderer that never becomes ready as a startup timeout", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("crypto", { randomUUID: vi.fn(() => "request-1") });
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));

    const result = renderMermaidDiagram("flowchart LR\nA --> B", theme);
    await vi.runAllTimersAsync();

    await expect(result).resolves.toEqual({ kind: "startup_timeout" });
  });
});
