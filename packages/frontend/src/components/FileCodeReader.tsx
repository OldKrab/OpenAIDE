import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { MessageSquare, Pencil, Quote, X } from "lucide-react";
import { highlightFileViewerLines } from "./fileViewerHighlight";
import type { ProjectDiffLine } from "@openaide/app-server-client";
import "../styles/app/file-code-reader.css";

export type CodeLine = ProjectDiffLine & { heading?: string };
/** A saved comment or bare quote on a line range of this reader. `start` and `end` index `lines`. */
export type CodeNote = { id: string; start: number; end: number; text: string; quoted: string };
export type CodeNoteDraft = Omit<CodeNote, "id">;

/**
 * Shared source and diff reader. It owns line selection, the comment box and the notes shown
 * under their lines as typed state; the owner decides where a saved note goes. Selecting a line
 * opens the box directly under it and another line stretches the range, so nothing floats over
 * the code. Native text selection is left alone for copying.
 */
export function FileCodeReader({
  lines,
  language,
  path,
  diff = false,
  baseLabel = "HEAD",
  focusLine,
  notes = noNotes,
  onSaveNote,
  onRemoveNote,
  submitLabel,
}: {
  lines: CodeLine[];
  language?: string | null;
  /** Path written into the quote reference. */
  path: string;
  diff?: boolean;
  /** Name of the side a diff compares against. */
  baseLabel?: string;
  focusLine?: number | null;
  notes?: CodeNote[];
  /** Saves a new note, or replaces the one named by `replaces`. Absent makes the reader read-only. */
  onSaveNote?: (note: CodeNoteDraft, replaces?: string) => void;
  onRemoveNote?: (id: string) => void;
  /** Overrides the action label where a comment starts something other than a Composer quote. */
  submitLabel?: string;
}) {
  const root = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const [range, setRange] = useState<{ anchor: number; end: number }>();
  const [text, setText] = useState("");
  const [editing, setEditing] = useState<string>();
  const [cursor, setCursor] = useState<number>();
  const last = lines.length - 1;
  const start = range ? Math.min(range.anchor, range.end, last) : -1;
  const end = range ? Math.min(Math.max(range.anchor, range.end), last) : -1;
  const tokens = useMemo(() => {
    const side = (skip: string) =>
      highlightFileViewerLines(
        lines
          .filter((line) => line.kind !== skip)
          .map((line) => line.text)
          .join("\n"),
        language,
      );
    const before = side("add");
    const after = side("remove");
    let old = 0;
    let next = 0;
    return lines.map((line) => {
      const value = line.kind === "remove" ? before[old] : after[next];
      if (line.kind !== "add") old++;
      if (line.kind !== "remove") next++;
      return value;
    });
  }, [lines, language]);
  // The line-number column is as wide as its longest number, not a fixed gutter.
  const digits = useMemo(
    () => String(lines.reduce((max, line) => Math.max(max, line.oldLine ?? 0, line.newLine ?? 0), 1)).length,
    [lines],
  );
  const noted = useMemo(() => {
    const marked = new Set<number>();
    for (const note of notes) for (let index = note.start; index <= Math.min(note.end, last); index++) marked.add(index);
    return marked;
  }, [notes, last]);
  useEffect(() => {
    if (focusLine)
      root.current
        ?.querySelector(`[data-index="${lines.findIndex((line) => line.newLine === focusLine)}"]`)
        ?.scrollIntoView({ block: "center" });
  }, [focusLine, lines]);
  // A pointer device gets the cursor straight away. On touch the keyboard waits for a tap on
  // the field, so the range can still be stretched first.
  useEffect(() => {
    if (!range || (range.anchor !== range.end && editing === undefined)) return;
    if (typeof matchMedia === "function" && matchMedia("(pointer: fine)").matches)
      field.current?.focus({ preventScroll: true });
  }, [range, editing]);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  }, [text, range]);

  function close() {
    setRange(undefined);
    setText("");
    setEditing(undefined);
  }
  function pick(index: number, extend = Boolean(range)) {
    if (!onSaveNote) return;
    setCursor(index);
    if (!range || !extend) setRange({ anchor: index, end: index });
    else if (start === end && index === start && !text && editing === undefined) close();
    else setRange({ anchor: range.anchor, end: index });
  }
  function edit(note: CodeNote) {
    setRange({ anchor: note.start, end: note.end });
    setText(note.text);
    setEditing(note.id);
  }
  function save() {
    if (!onSaveNote || start < 0) return;
    const selected = lines.slice(start, end + 1);
    const label = (values: number[]) =>
      values[0] === values.at(-1) ? String(values[0]) : `${values[0]}–${values.at(-1)}`;
    const old = selected.flatMap((line) => (line.oldLine === null ? [] : [line.oldLine]));
    const next = selected.flatMap((line) => (line.newLine === null ? [] : [line.newLine]));
    const reference = diff
      ? `${path} (diff; ${[old.length ? `${baseLabel} lines ${label(old)}` : "", next.length ? `working copy lines ${label(next)}` : ""].filter(Boolean).join("; ")})`
      : `${path}:${label(next)}`;
    const code = selected
      .map((line) => `${diff ? (line.kind === "add" ? "+ " : line.kind === "remove" ? "- " : "  ") : ""}${line.text}`)
      .join("\n");
    onSaveNote({ start, end, text: text.trim(), quoted: `${reference}\n${code}` }, editing);
    close();
  }
  /** Keyboard path to the same selection: arrows move a line cursor, Enter picks, Shift stretches. */
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    if (!onSaveNote || event.target !== event.currentTarget) return;
    const move = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (move) {
      event.preventDefault();
      const next = Math.max(0, Math.min(last, (cursor ?? (move > 0 ? -1 : lines.length)) + move));
      setCursor(next);
      if (event.shiftKey) setRange({ anchor: range?.anchor ?? cursor ?? next, end: next });
      root.current?.querySelector(`[data-index="${next}"]`)?.scrollIntoView?.({ block: "nearest" });
    } else if ((event.key === "Enter" || event.key === " ") && cursor !== undefined) {
      event.preventDefault();
      if (range) field.current?.focus();
      else pick(cursor);
    } else if (event.key === "Escape" && range) {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }
  const shown = range ? lines.slice(start, end + 1).map((line) => line.newLine ?? line.oldLine ?? 0) : [];
  const action =
    editing !== undefined ? "Save" : (submitLabel ?? (text.trim() ? "Add comment" : "Quote only"));

  return (
    <div
      ref={root}
      className="file-code-reader"
      data-diff={diff}
      data-language={language}
      data-selectable={Boolean(onSaveNote)}
      style={{ "--code-digits": digits } as CSSProperties}
    >
      <div
        className="file-viewer-source"
        tabIndex={onSaveNote ? 0 : undefined}
        role={onSaveNote ? "group" : undefined}
        aria-label={
          onSaveNote ? "Code lines. Arrow keys move, Enter selects a line, Shift with an arrow selects a range." : undefined
        }
        onKeyDown={keys}
      >
        {lines.map((line, index) => (
          <div key={index}>
            {line.heading && <div className="code-hunk-heading">{line.heading}</div>}
            <div
              className="file-viewer-line"
              data-index={index}
              data-kind={line.kind}
              data-focus={line.newLine === focusLine}
              data-selected={index >= start && index <= end}
              data-noted={noted.has(index)}
              data-cursor={cursor === index || undefined}
              onClick={(event) => {
                // A drag that selected text is a copy gesture, not a line pick.
                if (typeof window !== "undefined" && window.getSelection()?.isCollapsed === false) return;
                pick(index, Boolean(range) || event.shiftKey);
              }}
            >
              {diff ? (
                <>
                  <span className="file-viewer-gutter">{line.oldLine}</span>
                  <span className="file-viewer-gutter">{line.newLine}</span>
                  <span className="code-diff-sign">{line.kind === "add" ? "+" : line.kind === "remove" ? "−" : ""}</span>
                </>
              ) : (
                <span className="file-viewer-gutter">{line.newLine}</span>
              )}
              <span className="file-viewer-code">
                {tokens[index]?.map((span, i) => (
                  <span key={i} className={span.className}>
                    {span.text}
                  </span>
                ))}
              </span>
            </div>
            {notes
              .filter((note) => Math.min(note.end, last) === index && note.id !== editing)
              .map((note) => (
                <div key={note.id} className="code-inline">
                  {note.text ? (
                    <button type="button" className="code-comment" aria-label="Edit comment" onClick={() => edit(note)}>
                      <MessageSquare aria-hidden="true" size={14} />
                      <p>{note.text}</p>
                      <Pencil aria-hidden="true" size={13} />
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="code-quoted"
                      aria-label="Quoted lines: add a comment or remove"
                      onClick={() => edit(note)}
                    >
                      <Quote aria-hidden="true" size={12} />
                      Quoted
                    </button>
                  )}
                </div>
              ))}
            {index === end && (
              <div className="code-inline">
                <form
                  className="code-compose"
                  aria-label="Comment on the selected lines"
                  onSubmit={(event) => {
                    event.preventDefault();
                    save();
                  }}
                >
                  <div className="code-compose-head">
                    <strong>{shown.length === 1 ? `Line ${shown[0]}` : `${shown.length} lines`}</strong>
                    <span>Pick another line to select a range</span>
                    <button type="button" aria-label="Cancel" onClick={close}>
                      <X aria-hidden="true" size={16} />
                    </button>
                  </div>
                  <textarea
                    ref={field}
                    rows={1}
                    value={text}
                    aria-label="Comment"
                    placeholder="Comment for the agent…"
                    onChange={(event) => setText(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        event.preventDefault();
                        event.stopPropagation();
                        close();
                      }
                      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                        event.preventDefault();
                        save();
                      }
                    }}
                  />
                  <div className="code-compose-foot">
                    {editing !== undefined && onRemoveNote && (
                      <button
                        type="button"
                        className="code-compose-remove"
                        onClick={() => {
                          onRemoveNote(editing);
                          close();
                        }}
                      >
                        Remove
                      </button>
                    )}
                    <button type="submit">{action}</button>
                  </div>
                </form>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

const noNotes: CodeNote[] = [];
