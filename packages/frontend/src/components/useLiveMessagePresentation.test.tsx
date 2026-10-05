import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentMessagePart } from "@openaide/app-shell-contracts";
import { useLiveMessagePresentation } from "./useLiveMessagePresentation";

const PARAGRAPH = "streamed words arrive together ".repeat(50).trim();

function Row(props: { cursor?: string; text: string; urgent?: boolean }) {
  const parts: AgentMessagePart[] = [{ kind: "text", text: props.text }];
  const presentation = useLiveMessagePresentation({
    enabled: true,
    eventCursor: props.cursor,
    parts,
    urgent: props.urgent,
  });
  const part = presentation.parts[0];
  return <p>{part?.kind === "text" ? part.text : ""}</p>;
}

describe("useLiveMessagePresentation", () => {
  let tree: ReactTestRenderer;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      clearTimeout: globalThis.clearTimeout,
      setTimeout: globalThis.setTimeout,
    });
  });

  afterEach(() => {
    act(() => tree?.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const visible = () => (tree.toJSON() as { children: string[] | null }).children?.[0] ?? "";
  const render = (props: Parameters<typeof Row>[0]) => act(() => {
    if (tree) tree.update(<Row {...props} />);
    else tree = create(<Row {...props} />);
  });
  const advance = async (ms: number) => {
    for (let elapsed = 0; elapsed < ms; elapsed += 16) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(16);
      });
    }
  };
  const mountSettled = () => {
    tree = undefined as unknown as ReactTestRenderer;
    render({ text: "Start." });
  };

  it("types out a paragraph that arrived at once instead of showing it as a block", async () => {
    mountSettled();
    const text = `Start. ${PARAGRAPH}`;
    render({ cursor: "c1", text });

    await advance(128);
    const early = visible().length;
    expect(early).toBeGreaterThan("Start.".length);
    expect(early).toBeLessThan(text.length / 2);
    await advance(320);
    expect(visible().length).toBeGreaterThan(early);
    expect(visible().length).toBeLessThan(text.length);

    await advance(400);
    expect(visible()).toBe(text);
  });

  it("shows a small chunk almost as it arrives", async () => {
    mountSettled();
    render({ cursor: "c1", text: "Start. A few words." });

    await advance(112);
    expect(visible()).toBe("Start. A few words.");
  });

  it("ends a paragraph and the trickle after it within the lag cap of the last arrival", async () => {
    mountSettled();
    const paragraph = `Start. ${PARAGRAPH}`;
    render({ cursor: "c1", text: paragraph });
    await advance(400);
    render({ cursor: "c2", text: `${paragraph} more` });
    await advance(200);
    render({ cursor: "c3", text: `${paragraph} more text` });

    expect(visible().length).toBeGreaterThan(paragraph.length / 2);
    await advance(416);
    expect(visible()).toBe(`${paragraph} more text`);
  });

  it("spreads regular batches across the pauses between them", async () => {
    mountSettled();
    const batch = " twelve deltas arrive together as one batch of text every four hundred milliseconds";
    let text = "Start.";
    const stalls: number[] = [];
    for (let index = 0; index < 8; index += 1) {
      text += batch;
      render({ cursor: `c${index}`, text });
      // 400 ms between batches, sampled in 80 ms windows.
      for (let window = 0; window < 5; window += 1) {
        const before = visible().length;
        await advance(80);
        // The first batches teach the rhythm; after that the text never stops.
        if (index >= 3 && visible().length === before) stalls.push(index);
      }
    }

    expect(stalls).toEqual([]);
    await advance(800);
    expect(visible()).toBe(text);
  });

  it("stops trailing when the user has something to answer", async () => {
    mountSettled();
    const text = `Start. ${PARAGRAPH}`;
    render({ cursor: "c1", text });
    await advance(64);
    expect(visible().length).toBeLessThan(text.length / 2);

    render({ cursor: "c1", text, urgent: true });
    await advance(112);
    expect(visible()).toBe(text);
  });

  it("ends every frame on a whole word", async () => {
    mountSettled();
    const text = `Start. ${PARAGRAPH}`;
    render({ cursor: "c1", text });

    for (let frame = 0; frame < 52; frame += 1) {
      await advance(16);
      const shown = visible();
      expect(shown === text || text[shown.length] === " ").toBe(true);
    }
    expect(visible()).toBe(text);
  });

  it("shows text without a stream signal at once", () => {
    mountSettled();
    render({ text: `Start. ${PARAGRAPH}` });

    expect(visible()).toBe(`Start. ${PARAGRAPH}`);
  });

  it("shows text at once while the page is hidden", () => {
    vi.stubGlobal("document", {
      visibilityState: "hidden",
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    });
    mountSettled();
    render({ cursor: "c1", text: `Start. ${PARAGRAPH}` });

    expect(visible()).toBe(`Start. ${PARAGRAPH}`);
  });
});
