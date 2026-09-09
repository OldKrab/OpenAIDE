import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { MessageSquare, X } from "lucide-react";
import { highlightFileViewerLines } from "./fileViewerHighlight";
import type { ProjectDiffLine } from "@openaide/app-server-client";
import "../styles/app/file-code-reader.css";

export type CodeLine = ProjectDiffLine & { heading?: string };
type Selection = { anchor: number; end: number; text?: string };

/** Shared source/diff rendering owns line selection. DOM refs only locate native text
 * selection and measure its anchor; no external adapter mutates the reader's DOM. */
export function FileCodeReader({
  lines,
  language,
  path,
  diff = false,
  focusLine,
  onQuote,
}: {
  lines: CodeLine[];
  language?: string | null;
  path: string;
  diff?: boolean;
  focusLine?: number | null;
  onQuote?: (text: string) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);
  const toolbar = useRef<HTMLDivElement>(null);
  const anchor = useRef<number | undefined>(undefined);
  const drag = useRef(false);
  const [selection, setSelection] = useState<Selection>();
  const [selecting, setSelecting] = useState(false);
  const [extend, setExtend] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number }>();
  const start = selection ? Math.min(selection.anchor, selection.end) : -1;
  const end = selection ? Math.max(selection.anchor, selection.end) : -1;
  const tokens = useMemo(() => {
    const before = highlightFileViewerLines(
      lines
        .filter((line) => line.kind !== "add")
        .map((line) => line.text)
        .join("\n"),
      language,
    );
    const after = highlightFileViewerLines(
      lines
        .filter((line) => line.kind !== "remove")
        .map((line) => line.text)
        .join("\n"),
      language,
    );
    let old = 0;
    let next = 0;
    return lines.map((line) => {
      const value = line.kind === "remove" ? before[old] : after[next];
      if (line.kind !== "add") old++;
      if (line.kind !== "remove") next++;
      return value;
    });
  }, [lines, language]);
  useEffect(() => {
    setSelection(undefined);
    anchor.current = undefined;
  }, [lines, path]);
  useEffect(() => {
    if (focusLine)
      rowRefs.current[lines.findIndex((line) => line.newLine === focusLine)]?.scrollIntoView({ block: "center" });
  }, [focusLine, lines]);
  useEffect(() => {
    if (typeof document === "undefined") return;
    const changed = () => {
      if (drag.current) return;
      const current = window.getSelection();
      if (!current || current.isCollapsed) {
        setSelection((previous) => (previous?.text !== undefined ? undefined : previous));
        return;
      }
      const index = (node: Node | null) => rowRefs.current.findIndex((row) => node && row?.contains(node));
      const first = index(current.anchorNode);
      const last = index(current.focusNode);
      if (first >= 0 && last >= 0) {
        anchor.current = first;
        setSelection({ anchor: first, end: last, text: current.toString() });
      }
    };
    const move = (event: PointerEvent) => {
      if (!drag.current) return;
      const element = document.elementFromPoint(event.clientX, event.clientY);
      const index = rowRefs.current.findIndex((row) => element && row?.contains(element));
      if (index >= 0) setSelection({ anchor: anchor.current ?? index, end: index });
    };
    const finish = () => {
      drag.current = false;
      setSelecting(false);
    };
    document.addEventListener("selectionchange", changed);
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", finish);
    document.addEventListener("pointercancel", finish);
    return () => {
      document.removeEventListener("selectionchange", changed);
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", finish);
      document.removeEventListener("pointercancel", finish);
    };
  }, []);
  useLayoutEffect(() => {
    if (!selection || selecting || !toolbar.current || !root.current) {
      setPosition(undefined);
      return;
    }
    const place = () => {
      const viewport = root.current!.getBoundingClientRect();
      const first = rowRefs.current[start]?.getBoundingClientRect();
      const last = rowRefs.current[end]?.getBoundingClientRect();
      if (!first || !last || last.bottom < viewport.top || first.top > viewport.bottom) {
        setPosition(undefined);
        return;
      }
      let rect = rowRefs.current[selection.end]!.getBoundingClientRect();
      let x = viewport.left + 84;
      if (selection.text !== undefined) {
        const native = window.getSelection();
        const rects = native?.rangeCount ? [...native.getRangeAt(0).getClientRects()] : [];
        rect = rects.at(-1) ?? rect;
        x = rect.left;
      }
      const width = toolbar.current!.offsetWidth;
      const height = toolbar.current!.offsetHeight;
      const below = rect.bottom + 6;
      setPosition({
        left: Math.max(viewport.left + 8, Math.min(x, viewport.right - width - 8)),
        top: Math.max(
          viewport.top + 6,
          Math.min(below + height < viewport.bottom ? below : rect.top - height - 6, viewport.bottom - height - 6),
        ),
      });
    };
    place();
    root.current.addEventListener("scroll", place);
    window.addEventListener("resize", place);
    const surface = root.current;
    return () => {
      surface.removeEventListener("scroll", place);
      window.removeEventListener("resize", place);
    };
  }, [selection, selecting, start, end, extend]);
  function choose(index: number, shift: boolean) {
    if (typeof window !== "undefined") window.getSelection()?.removeAllRanges();
    const first = (shift || extend) && anchor.current !== undefined ? anchor.current : index;
    anchor.current = first;
    setSelection({ anchor: first, end: index });
    setExtend(false);
  }
  function clear() {
    setSelection(undefined);
    anchor.current = undefined;
    setExtend(false);
    if (typeof window !== "undefined") window.getSelection()?.removeAllRanges();
  }
  function comment() {
    if (!selection) return;
    const selected = lines.slice(start, end + 1);
    const label = (values: number[]) =>
      values[0] === values.at(-1) ? String(values[0]) : `${values[0]}–${values.at(-1)}`;
    const old = selected.flatMap((line) => (line.oldLine === null ? [] : [line.oldLine]));
    const next = selected.flatMap((line) => (line.newLine === null ? [] : [line.newLine]));
    const reference = diff
      ? `${path} (diff; ${[old.length ? `HEAD lines ${label(old)}` : "", next.length ? `working copy lines ${label(next)}` : ""].filter(Boolean).join("; ")})`
      : `${path}:${label(next)}`;
    const text =
      selection.text ??
      selected
        .map((line) => `${diff ? (line.kind === "add" ? "+ " : line.kind === "remove" ? "- " : "  ") : ""}${line.text}`)
        .join("\n");
    onQuote?.(`${reference}\n${text}`);
    clear();
  }
  function number(value: number | null, index: number, side: string) {
    return value === null ? null : (
      <button
        className="code-line-number"
        aria-label={`Select ${side}line ${value}`}
        aria-pressed={selection?.text === undefined && index >= start && index <= end}
        title="Select line · Drag or Shift-click for a range"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          drag.current = true;
          event.currentTarget.focus();
          choose(index, event.shiftKey);
        }}
        onClick={(event) => {
          if (event.detail === 0) choose(index, event.shiftKey);
        }}
        onKeyDown={(event) => {
          if (event.shiftKey && ["ArrowUp", "ArrowDown"].includes(event.key)) {
            event.preventDefault();
            const next = Math.max(0, Math.min(lines.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)));
            choose(next, true);
            rowRefs.current[next]?.querySelector<HTMLButtonElement>("button")?.focus();
          }
        }}
      >
        {value}
      </button>
    );
  }
  return (
    <div
      ref={root}
      className="file-code-reader"
      data-diff={diff}
      data-language={language}
      onPointerDown={(event) => {
        if (!(event.target as HTMLElement).closest(".code-selection-actions")) setSelecting(true);
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && selection) {
          event.preventDefault();
          event.stopPropagation();
          clear();
        }
      }}
    >
      <div className="file-viewer-source">
        {lines.map((line, index) => (
          <div key={index}>
            {line.heading && <div className="code-hunk-heading">{line.heading}</div>}
            <div
              ref={(element) => {
                rowRefs.current[index] = element;
              }}
              className="file-viewer-line"
              data-kind={line.kind}
              data-focus={line.newLine === focusLine}
              data-selected={selection?.text === undefined && index >= start && index <= end}
            >
              {diff ? (
                <>
                  <span className="file-viewer-gutter">{number(line.oldLine, index, "HEAD ")}</span>
                  <span className="file-viewer-gutter">{number(line.newLine, index, "working copy ")}</span>
                  <span className="code-diff-sign">
                    {line.kind === "add" ? "+" : line.kind === "remove" ? "−" : ""}
                  </span>
                </>
              ) : (
                <span className="file-viewer-gutter">{number(line.newLine, index, "")}</span>
              )}
              <span className="file-viewer-code">
                {tokens[index]?.map((span, i) => (
                  <span key={i} className={span.className}>
                    {span.text}
                  </span>
                ))}
              </span>
            </div>
          </div>
        ))}
      </div>
      {selection && onQuote && (
        <div
          ref={toolbar}
          className="code-selection-actions"
          role="group"
          aria-label="Selection actions"
          onPointerDown={(event) => event.preventDefault()}
          style={{
            left: position?.left ?? 0,
            top: position?.top ?? 0,
            visibility: position && !selecting ? "visible" : "hidden",
          }}
        >
          <span aria-live="polite">
            {selection.text !== undefined
              ? "Text selected"
              : `${end - start + 1} ${start === end ? "line" : "lines"} selected`}
          </span>
          <button className="code-select-range" aria-pressed={extend} onClick={() => setExtend(!extend)}>
            {extend ? "Choose end line…" : "Select range"}
          </button>
          <button aria-label="Clear line selection" onClick={clear}>
            <X size={14} />
          </button>
          <button aria-label="Comment" onClick={comment}>
            <MessageSquare size={14} />
            Comment
          </button>
        </div>
      )}
    </div>
  );
}
