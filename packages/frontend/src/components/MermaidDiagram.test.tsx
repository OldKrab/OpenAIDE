import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";

const renderMermaidDiagram = vi.hoisted(() => vi.fn());
const copyText = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const theme = vi.hoisted(() => ({
  background: "rgb(20, 20, 20)",
  border: "rgb(80, 80, 80)",
  highContrast: false,
  muted: "rgb(140, 140, 140)",
  panel: "rgb(30, 30, 30)",
  primary: "rgb(80, 140, 240)",
  text: "rgb(230, 230, 230)",
  fontFamily: "sans-serif",
}));

vi.mock("../mermaid/renderService", () => ({ renderMermaidDiagram }));
vi.mock("../mermaid/useMermaidTheme", () => ({ useMermaidTheme: () => theme }));
vi.mock("./clipboard", () => ({ copyText }));

import { MermaidDiagram } from "./MermaidDiagram";

describe("MermaidDiagram", () => {
  afterEach(() => {
    renderMermaidDiagram.mockReset();
    copyText.mockClear();
  });

  it("keeps source visible until rendering succeeds, then exposes diagram controls", async () => {
    let resolveRender!: (value: unknown) => void;
    renderMermaidDiagram.mockReturnValue(new Promise((resolve) => { resolveRender = resolve; }));
    let tree!: ReactTestRenderer;
    act(() => { tree = create(<MermaidDiagram source={"flowchart LR\n  A --> B"} />); });

    expect(tree.root.findAllByProps({ className: "agent-mermaid-image" })).toHaveLength(0);
    expect(tree.root.findByProps({ children: "Rendering diagram…" })).toBeTruthy();
    expect(tree.root.findByType("code").children.join("")).toContain("A --> B");

    await act(async () => {
      resolveRender({
        kind: "success",
        url: "data:image/svg+xml,diagram",
        title: "Build flow",
        description: "Build moves from A to B.",
      });
      await Promise.resolve();
    });

    expect(tree.root.findByProps({ className: "agent-mermaid-image" }).props.alt).toBe("Build flow");
    expect(tree.root.findByProps({ children: "View source" })).toBeTruthy();
    expect(tree.root.findByProps({ children: "Expand diagram" })).toBeTruthy();
    const copy = tree.root.findByProps({ "aria-label": "Copy source" });
    expect(copy.props.className).toBe("attachment-preview-action");
    expect(copy.findByType("span").props.className).toBe("visually-hidden");
  });

  it("distinguishes a slow renderer startup from a diagram render failure", async () => {
    renderMermaidDiagram.mockResolvedValue({ kind: "startup_timeout" });
    let tree!: ReactTestRenderer;
    await act(async () => { tree = create(<MermaidDiagram source="flowchart LR\nA --> B" />); });

    expect(tree.root.findByProps({ children: "Diagram renderer took too long to start" })).toBeTruthy();
    expect(tree.root.findByProps({ children: "Retry" })).toBeTruthy();
  });

  it("never leaves a visible Diagram rendering indefinitely while queued", async () => {
    vi.useFakeTimers();
    renderMermaidDiagram.mockReturnValue(new Promise(() => undefined));
    let tree!: ReactTestRenderer;
    act(() => { tree = create(<MermaidDiagram source="flowchart LR\nA --> B" />); });

    expect(tree.root.findByProps({ children: "Rendering diagram…" })).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(15_000); });

    expect(tree.root.findByProps({ children: "Diagram renderer took too long to start" })).toBeTruthy();
    expect(tree.root.findByProps({ children: "Retry" })).toBeTruthy();
  });

  it("preserves invalid source with a stable non-retryable status", async () => {
    renderMermaidDiagram.mockResolvedValue({ kind: "invalid" });
    let tree!: ReactTestRenderer;
    await act(async () => { tree = create(<MermaidDiagram source="not a graph" />); });

    expect(tree.root.findByProps({ children: "Diagram could not be rendered" })).toBeTruthy();
    expect(tree.root.findAllByProps({ children: "Retry" })).toHaveLength(0);
    expect(tree.root.findByType("code").children.join("")).toBe("not a graph");
  });

  it("retries a timed-out render only after explicit user action", async () => {
    renderMermaidDiagram
      .mockResolvedValueOnce({ kind: "timeout" })
      .mockResolvedValueOnce({ kind: "success", url: "data:image/svg+xml,retried" });
    let tree!: ReactTestRenderer;
    await act(async () => { tree = create(<MermaidDiagram source="flowchart LR\nA --> B" />); });

    expect(renderMermaidDiagram).toHaveBeenCalledTimes(1);
    await act(async () => { tree.root.findByProps({ children: "Retry" }).parent!.props.onClick(); });

    expect(renderMermaidDiagram).toHaveBeenCalledTimes(2);
    expect(tree.root.findAllByProps({ className: "agent-mermaid-image" })).toHaveLength(1);
  });
});
