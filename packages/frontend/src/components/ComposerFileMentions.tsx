import { FileText, LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";

export type FileMentionToken = {
  end: number;
  query: string;
  start: number;
};

export type FileMentionPickerState = {
  activeIndex: number;
  error?: string;
  loading: boolean;
  paths: string[];
  refreshing: boolean;
  token: FileMentionToken;
};

/** Finds an active @ token only at prompt boundaries where completion is unambiguous. */
export function fileMentionTokenAtCursor(text: string, cursor: number): FileMentionToken | undefined {
  if (cursor < 0 || cursor > text.length) return undefined;
  let start = cursor;
  while (start > 0 && !/\s/.test(text[start - 1])) start -= 1;
  if (text[start] !== "@" || (start > 0 && !/\s/.test(text[start - 1]))) return undefined;
  const fragment = text.slice(start + 1, cursor);
  if (fragment.startsWith('"')) {
    if (fragment.slice(1).includes('"')) return undefined;
    return { start, end: cursor, query: fragment.slice(1) };
  }
  if (/\s/.test(fragment)) return undefined;
  return { start, end: cursor, query: fragment };
}

export function fileMentionRanges(text: string): Array<{ start: number; end: number }> {
  const ranges: Array<{ start: number; end: number }> = [];
  const pattern = /(^|\s)(@"[^"\n]+"|@[^\s@]+)/g;
  for (const match of text.matchAll(pattern)) {
    const prefix = match[1].length;
    const start = (match.index ?? 0) + prefix;
    ranges.push({ start, end: start + match[2].length });
  }
  return ranges;
}

export function replaceFileMention(text: string, token: FileMentionToken, path: string) {
  const mention = /\s/.test(path) ? `@"${path}"` : `@${path}`;
  const tail = text.slice(token.end);
  const suffix = tail.length === 0 ? " " : "";
  return {
    text: `${text.slice(0, token.start)}${mention}${suffix}${tail}`,
    cursor: token.start + mention.length + suffix.length,
  };
}

export function useFileMentionPicker(
  browser: TaskFileBrowserCallbacks | undefined,
  token: FileMentionToken | undefined,
) {
  const [state, setState] = useState<FileMentionPickerState | undefined>();
  useEffect(() => {
    if (!browser || !token) {
      setState(undefined);
      return undefined;
    }
    let current = true;
    setState({ activeIndex: 0, loading: true, paths: [], refreshing: false, token });
    const timer = globalThis.setTimeout(() => {
      void browser.searchFiles(token.query).then((result) => {
        if (!current) return;
        setState({
          activeIndex: 0,
          error: result.state === "unavailable"
            ? result.notice ?? "Workspace files are unavailable."
            : undefined,
          loading: false,
          paths: result.paths,
          refreshing: result.state === "refreshing",
          token,
        });
      }, (error: unknown) => {
        if (!current) return;
        setState({
          activeIndex: 0,
          error: error instanceof Error ? error.message : "Workspace files are unavailable.",
          loading: false,
          paths: [],
          refreshing: false,
          token,
        });
      });
    }, 75);
    return () => {
      current = false;
      globalThis.clearTimeout(timer);
    };
  }, [browser?.ownerKey, token?.start, token?.end, token?.query]);
  return [state, setState] as const;
}

export function FileMentionPicker({ state, onSelect }: {
  state: FileMentionPickerState;
  onSelect: (path: string) => void;
}) {
  return (
    <div aria-label="Workspace files" className="composer-file-popover" role="listbox">
      {state.loading ? (
        <div className="composer-file-status" role="status">
          <LoaderCircle aria-hidden="true" size={13} /> Indexing files…
        </div>
      ) : state.error ? (
        <div className="composer-file-status composer-file-error" role="status">{state.error}</div>
      ) : state.paths.length === 0 ? (
        <div className="composer-file-status" role="status">No matching files</div>
      ) : state.paths.map((path, index) => (
        <button
          aria-selected={index === state.activeIndex}
          className="composer-file-option"
          key={path}
          onClick={() => onSelect(path)}
          onMouseDown={(event) => event.preventDefault()}
          role="option"
          type="button"
        >
          <FileText aria-hidden="true" size={13} />
          <span>{path}</span>
        </button>
      ))}
      {state.refreshing ? <div className="composer-file-refresh" role="status">Refreshing files…</div> : null}
    </div>
  );
}
