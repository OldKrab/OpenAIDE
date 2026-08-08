import { Quote } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { quoteableSelectionText } from "./quoteSelection";

type QuoteSelection = {
  source: HTMLElement;
  text: string;
  x: number;
  y: number;
  placement: "above" | "below";
  viewportWidth: number;
};

/**
 * Keeps DOM-selection mechanics at the Chat boundary. The rest of the Task
 * surface receives only ordinary, already-qualified text to append to draft.
 */
export function QuoteSelectionAction({
  onQuote,
  root,
}: {
  onQuote: (text: string) => void;
  root: HTMLElement | null;
}) {
  const [selection, setSelection] = useState<QuoteSelection | undefined>();
  const [actionWidth, setActionWidth] = useState(0);
  const selectingWithPointer = useRef(false);
  const actionRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!root) return undefined;
    const document = root.ownerDocument;
    // Virtualized server-style test surfaces have no browser selection API.
    // They render Chat structure but cannot host an interactive Quote action.
    if (!document || typeof document.addEventListener !== "function" || typeof document.getSelection !== "function") {
      return undefined;
    }
    const dismiss = () => setSelection(undefined);
    const update = () => {
      if (selectingWithPointer.current) return;
      setSelection(readQuoteSelection(root));
    };
    const settlePointerSelection = () => {
      selectingWithPointer.current = false;
      update();
    };
    const beginPointerSelection = () => {
      selectingWithPointer.current = true;
      dismiss();
    };
    const dismissOutside = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (root.contains(target) || actionRef.current?.contains(target)) return;
      dismiss();
    };
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") dismiss();
    };

    document.addEventListener("selectionchange", update);
    document.addEventListener("pointerdown", dismissOutside, true);
    document.addEventListener("keydown", dismissOnEscape);
    root.addEventListener("pointerdown", beginPointerSelection);
    root.addEventListener("pointerup", settlePointerSelection);
    root.addEventListener("pointercancel", dismiss);
    return () => {
      document.removeEventListener("selectionchange", update);
      document.removeEventListener("pointerdown", dismissOutside, true);
      document.removeEventListener("keydown", dismissOnEscape);
      root.removeEventListener("pointerdown", beginPointerSelection);
      root.removeEventListener("pointerup", settlePointerSelection);
      root.removeEventListener("pointercancel", dismiss);
    };
  }, [root]);

  useEffect(() => {
    if (!selection) return undefined;
    // A streaming source can mutate after selection; a Quote must capture only
    // text the user still sees at activation time.
    const observer = new MutationObserver(() => setSelection(undefined));
    observer.observe(selection.source, { childList: true, characterData: true, subtree: true });
    return () => observer.disconnect();
  }, [selection]);

  useLayoutEffect(() => {
    if (!selection) return;
    const width = actionRef.current?.getBoundingClientRect().width ?? 0;
    if (width > 0) setActionWidth((current) => current === width ? current : width);
  }, [selection]);

  if (!selection) return null;
  // Fractional text metrics can round a mathematically flush edge just outside
  // the viewport, so retain one physical pixel of room on either side.
  const edgeInset = 1;
  const minimumX = actionWidth / 2 + edgeInset;
  const x = actionWidth > 0
    ? clamp(selection.x, minimumX, Math.max(minimumX, selection.viewportWidth - minimumX))
    : selection.x;
  return (
    <button
      aria-label="Quote selected text"
      className="quote-selection-action"
      data-placement={selection.placement}
      onClick={() => {
        onQuote(selection.text);
        setSelection(undefined);
      }}
      onPointerDown={(event) => event.preventDefault()}
      ref={actionRef}
      style={{ left: x, top: selection.y }}
      type="button"
    >
      <Quote aria-hidden="true" size={14} />
      <span>Quote</span>
    </button>
  );
}

function readQuoteSelection(root: HTMLElement): QuoteSelection | undefined {
  const selection = root.ownerDocument.getSelection();
  if (!selection || selection.rangeCount !== 1 || selection.isCollapsed) return undefined;
  const range = selection.getRangeAt(0);
  const startSource = quoteSourceForNode(range.startContainer);
  const endSource = quoteSourceForNode(range.endContainer);
  if (!startSource || startSource !== endSource || !root.contains(startSource)) return undefined;
  const text = quoteableSelectionText(selection.toString());
  if (!text) return undefined;
  // Range end follows document order. Selection focus is the actual endpoint
  // where a pointer or keyboard selection finished, including backward drags.
  const rect = selectionFocusRect(selection, range, root.ownerDocument)
    ?? range.getBoundingClientRect?.()
    ?? startSource.getBoundingClientRect();
  const viewportWidth = root.ownerDocument.defaultView?.innerWidth ?? 0;
  const placement = rect.top < 44 ? "below" : "above";
  return {
    source: startSource,
    text,
    x: rect.left + rect.width / 2,
    y: placement === "above" ? Math.max(8, rect.top - 8) : rect.bottom + 8,
    placement,
    viewportWidth,
  };
}

function selectionFocusRect(selection: Selection, selectionRange: Range, document: Document) {
  if (!selection.focusNode) return undefined;
  const focusRange = document.createRange();
  try {
    focusRange.setStart(selection.focusNode, selection.focusOffset);
    focusRange.collapse(true);
    const focusRect = focusRange.getBoundingClientRect?.();
    if (focusRect && focusRect.height > 0) return focusRect;
    return selectionRangeEndpointRect(selection, selectionRange);
  } catch {
    // A DOM mutation can invalidate the selection between qualification and
    // geometry lookup. Fall back to the selection range for this paint only.
    return undefined;
  }
}

function selectionRangeEndpointRect(selection: Selection, range: Range) {
  const rects = Array.from(range.getClientRects?.() ?? []);
  if (!rects.length) return undefined;
  const focusStartsRange = selection.focusNode === range.startContainer
    && selection.focusOffset === range.startOffset;
  const lineRect = focusStartsRange ? rects[0] : rects.at(-1)!;
  const x = focusStartsRange ? lineRect.left : lineRect.right;
  return {
    bottom: lineRect.bottom,
    height: lineRect.height,
    left: x,
    right: x,
    top: lineRect.top,
    width: 0,
    x,
    y: lineRect.y,
  } as DOMRect;
}

function quoteSourceForNode(node: Node) {
  const element = node.nodeType === Node.ELEMENT_NODE
    ? node as Element
    : node.parentElement;
  return element?.closest<HTMLElement>("[data-quote-source]");
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}
