// @vitest-environment jsdom

import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { QuoteSelectionAction } from "./QuoteSelectionAction";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
let restoreRangeRect: (() => void) | undefined;

afterEach(() => {
  root?.unmount();
  root = undefined;
  restoreRangeRect?.();
  restoreRangeRect = undefined;
  document.getSelection()?.removeAllRanges();
  document.body.replaceChildren();
});

it("quotes a qualifying selection from one rendered message body", async () => {
  const onQuote = vi.fn();
  mount(<QuoteTestSurface onQuote={onQuote} />);
  const text = document.querySelector<HTMLElement>("[data-quote-source]")!.firstChild!;

  select(text, 0, 5);
  await act(async () => document.dispatchEvent(new Event("selectionchange")));

  const quote = document.querySelector<HTMLButtonElement>('[aria-label="Quote selected text"]');
  expect(quote).not.toBeNull();
  await act(async () => quote!.click());
  expect(onQuote).toHaveBeenCalledWith("Alpha");
});

it("does not offer Quote for short or cross-message selections", async () => {
  mount(<QuoteTestSurface onQuote={vi.fn()} />);
  const sources = document.querySelectorAll<HTMLElement>("[data-quote-source]");
  const first = sources[0].firstChild!;
  const second = sources[1].firstChild!;

  select(first, 0, 2);
  await act(async () => document.dispatchEvent(new Event("selectionchange")));
  expect(document.querySelector('[aria-label="Quote selected text"]')).toBeNull();

  const range = document.createRange();
  range.setStart(first, 0);
  range.setEnd(second, 5);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  await act(async () => document.dispatchEvent(new Event("selectionchange")));
  expect(document.querySelector('[aria-label="Quote selected text"]')).toBeNull();
});

it("waits for pointer selection to settle before showing Quote", async () => {
  mount(<QuoteTestSurface onQuote={vi.fn()} />);
  const source = document.querySelector<HTMLElement>("[data-quote-source]")!;
  const messageList = source.parentElement!;
  const text = source.firstChild!;

  await act(async () => messageList.dispatchEvent(new Event("pointerdown", { bubbles: true })));
  select(text, 0, 5);
  await act(async () => document.dispatchEvent(new Event("selectionchange")));
  expect(document.querySelector('[aria-label="Quote selected text"]')).toBeNull();

  await act(async () => messageList.dispatchEvent(new Event("pointerup", { bubbles: true })));
  expect(document.querySelector('[aria-label="Quote selected text"]')).not.toBeNull();
});

it("anchors Quote above the selection endpoint instead of its middle", async () => {
  mockRangeRects({
    collapsed: rect(240, 200, 0, 20),
    selection: rect(40, 60, 200, 20),
  });
  mount(<QuoteTestSurface onQuote={vi.fn()} />);
  const text = document.querySelector<HTMLElement>("[data-quote-source]")!.firstChild!;

  select(text, 0, 5);
  await act(async () => document.dispatchEvent(new Event("selectionchange")));

  const quote = document.querySelector<HTMLButtonElement>('[aria-label="Quote selected text"]')!;
  expect(quote.style.left).toBe("240px");
  expect(quote.style.top).toBe("192px");
  expect(quote.dataset.placement).toBe("above");
});

function QuoteTestSurface({ onQuote }: { onQuote: (text: string) => void }) {
  const [messageList, setMessageList] = useState<HTMLDivElement | null>(null);
  return (
    <>
      <div ref={setMessageList}>
        <p data-quote-source="user">Alpha beta</p>
        <p data-quote-source="agent">Gamma delta</p>
      </div>
      <QuoteSelectionAction onQuote={onQuote} root={messageList} />
    </>
  );
}

function mount(element: React.ReactNode) {
  const container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  act(() => root!.render(element));
}

function select(node: Node, start: number, end: number) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
}

function mockRangeRects({ collapsed, selection }: { collapsed: DOMRect; selection: DOMRect }) {
  const descriptor = Object.getOwnPropertyDescriptor(Range.prototype, "getBoundingClientRect");
  Object.defineProperty(Range.prototype, "getBoundingClientRect", {
    configurable: true,
    value(this: Range) {
      return this.collapsed ? collapsed : selection;
    },
  });
  restoreRangeRect = () => {
    if (descriptor) Object.defineProperty(Range.prototype, "getBoundingClientRect", descriptor);
    else Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
  };
}

function rect(left: number, top: number, width: number, height: number) {
  return {
    bottom: top + height,
    height,
    left,
    right: left + width,
    top,
    width,
    x: left,
    y: top,
  } as DOMRect;
}
