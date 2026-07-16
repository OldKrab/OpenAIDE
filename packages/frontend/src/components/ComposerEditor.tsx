import { forwardRef, memo, useImperativeHandle, useLayoutEffect, useRef } from "react";
import type { ClipboardEvent, KeyboardEvent, MutableRefObject } from "react";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { exactSlashCommandMatches } from "./commandSearch";
import { fileMentionRanges } from "./ComposerFileMentions";
import {
  captureFocusedEditorSelection,
  restoreEditorSelection,
  selectionOffsets,
  setSelectionOffsets,
  type EditorSelection,
} from "./composerEditorSelection";

export { captureFocusedEditorSelection, restoreEditorSelection } from "./composerEditorSelection";

export type ComposerEditorHandle = {
  focus: () => void;
  selectionRange: () => { start: number; end: number };
  selectionStart: () => number;
  setSelectionRange: (start: number, end: number) => void;
};

type ComposerEditorProps = {
  ariaLabel: string;
  commandCatalog?: AgentCommandsCatalog;
  disabled: boolean;
  onInputText: (value: string, cursor: number) => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onPaste: (event: ClipboardEvent<HTMLDivElement>) => void;
  onPointerDown: () => void;
  placeholder: string;
  renderRevision: number;
  value: string;
};

export const ComposerEditor = forwardRef<ComposerEditorHandle, ComposerEditorProps>(function ComposerEditor({
  ariaLabel,
  commandCatalog,
  disabled,
  onInputText,
  onKeyDown,
  onPaste,
  onPointerDown,
  placeholder,
  renderRevision,
  value,
}, ref) {
  const handlersRef = useRef<ComposerEditorHandlers>({
    onInputText,
    onKeyDown,
    onPaste,
    onPointerDown,
  });
  handlersRef.current = {
    onInputText,
    onKeyDown,
    onPaste,
    onPointerDown,
  };

  return (
    <ComposerEditorSurface
      ariaLabel={ariaLabel}
      disabled={disabled}
      handlersRef={handlersRef}
      html={renderEditorHtml(value, commandCatalog)}
      placeholder={placeholder}
      renderRevision={renderRevision}
      ref={ref}
      valueLength={value.length}
    />
  );
});

type ComposerEditorHandlers = Pick<
  ComposerEditorProps,
  "onInputText" | "onKeyDown" | "onPaste" | "onPointerDown"
>;

type ComposerEditorSurfaceProps = {
  ariaLabel: string;
  disabled: boolean;
  handlersRef: MutableRefObject<ComposerEditorHandlers>;
  html: string;
  placeholder: string;
  renderRevision: number;
  valueLength: number;
};

const ComposerEditorSurface = memo(forwardRef<ComposerEditorHandle, ComposerEditorSurfaceProps>(function ComposerEditorSurface({
  ariaLabel,
  disabled,
  handlersRef,
  html,
  placeholder,
  renderRevision,
  valueLength,
}, ref) {
  const editorRef = useRef<HTMLDivElement | null>(null);
  const restoreSelectionRef = useRef<EditorSelection | undefined>(undefined);

  useImperativeHandle(ref, () => ({
    focus: () => editorRef.current?.focus(),
    selectionRange: () => editorRef.current ? selectionOffsets(editorRef.current) : { start: valueLength, end: valueLength },
    selectionStart: () => editorRef.current ? selectionOffsets(editorRef.current).start : valueLength,
    setSelectionRange: (start, end) => {
      const editor = editorRef.current;
      if (editor) setSelectionOffsets(editor, start, end);
    },
  }), [valueLength]);

  useLayoutEffect(() => {
    const editor = editorRef.current;
    // Keep React from rewriting the focused contenteditable before selection can be captured.
    if (editor && editor.innerHTML !== html) editor.innerHTML = html;
  }, [html, renderRevision]);

  useLayoutEffect(() => {
    const selection = restoreSelectionRef.current;
    const editor = editorRef.current;
    restoreSelectionRef.current = undefined;
    if (selection && editor) restoreEditorSelection(editor, selection);
    return () => {
      restoreSelectionRef.current = captureFocusedEditorSelection(editorRef.current);
    };
  });

  return (
    <div
      aria-disabled={disabled}
      aria-label={ariaLabel}
      aria-placeholder={placeholder}
      className="composer-editor"
      contentEditable={!disabled}
      data-empty={valueLength === 0 ? true : undefined}
      data-placeholder={placeholder}
      onInput={(event) => {
        const nextValue = editableText(event.currentTarget);
        event.currentTarget.toggleAttribute("data-empty", nextValue.length === 0);
        const cursor = selectionOffsets(event.currentTarget).end;
        restoreSelectionRef.current = { start: cursor, end: cursor };
        handlersRef.current.onInputText(nextValue, cursor);
      }}
      onKeyDown={(event) => handlersRef.current.onKeyDown(event)}
      onPaste={(event) => handlersRef.current.onPaste(event)}
      onPointerDown={() => handlersRef.current.onPointerDown()}
      ref={editorRef}
      role="textbox"
      suppressContentEditableWarning
    />
  );
}), sameEditorSurfaceProps);

function sameEditorSurfaceProps(previous: ComposerEditorSurfaceProps, next: ComposerEditorSurfaceProps) {
  return previous.ariaLabel === next.ariaLabel
    && previous.disabled === next.disabled
    && previous.html === next.html
    && previous.placeholder === next.placeholder
    && previous.renderRevision === next.renderRevision
    && previous.valueLength === next.valueLength;
}

export function renderEditorHtml(text: string, commandCatalog: AgentCommandsCatalog | undefined) {
  const commandMatches = exactSlashCommandMatches(text, commandCatalog?.commands).map((match) => ({
    end: match.token.end,
    html: () => {
      const label = text.slice(match.token.start, match.token.end);
      const hint = match.command.input_hint ? ` Argument: ${match.command.input_hint}.` : "";
      return renderReferenceToken("command", label, `${label}: ${match.command.description}${hint}`);
    },
    start: match.token.start,
  }));
  const fileMatches = fileMentionRanges(text).map((range) => ({
    ...range,
    html: () => renderReferenceToken("file", text.slice(range.start, range.end), "Workspace file"),
  }));
  const matches = [...commandMatches, ...fileMatches].sort((left, right) => left.start - right.start);
  if (!matches.length) return renderPlainTextHtml(text);

  const nodes: string[] = [];
  let cursor = 0;
  for (const match of matches) {
    if (match.start < cursor) continue;
    if (match.start > cursor) nodes.push(renderPlainTextHtml(text.slice(cursor, match.start)));
    nodes.push(match.html());
    cursor = match.end;
  }
  if (cursor < text.length) nodes.push(renderPlainTextHtml(text.slice(cursor)));
  return nodes.join("");
}

function renderReferenceToken(kind: "command" | "file", value: string, title: string) {
  const [sigil, ...label] = value;
  return `<span class="composer-reference-token composer-${kind}-token" spellcheck="false" title="${escapeHtml(title)}"><span class="composer-reference-sigil">${escapeHtml(sigil ?? "")}</span><span class="composer-reference-label">${escapeHtml(label.join(""))}</span></span>`;
}

function renderPlainTextHtml(text: string) {
  const html = text.split("\n").map(escapeHtml).join("<br>");
  // A trailing BR is only a caret marker in contenteditable; a second BR makes the empty line visible.
  return text.endsWith("\n") ? `${html}<br>` : html;
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => {
    switch (char) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case "\"":
        return "&quot;";
      case "'":
        return "&#39;";
      default:
        return char;
    }
  });
}

function editableText(root: HTMLElement) {
  const text = root.innerText ?? root.textContent ?? "";
  return text.replace(/\n$/, "");
}
