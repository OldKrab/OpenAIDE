import {
  Braces,
  Code2,
  Database,
  File,
  FileArchive,
  FileCog,
  FileImage,
  FileText,
  LoaderCircle,
} from "lucide-react";
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
  ownerKey: string;
  paths: string[];
  token: FileMentionToken;
};

export type FileIconKind =
  | "archive" | "config" | "database" | "file" | "image" | "javascript"
  | "json" | "markdown" | "python" | "rust" | "text" | "typescript" | "web";

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

export function fileIconKind(path: string): FileIconKind {
  const name = path.split("/").pop()?.toLowerCase() ?? path.toLowerCase();
  if (["license", "notice", "readme"].includes(name)) return "text";
  if (["dockerfile", "makefile", "cargo.toml"].includes(name)) return "config";
  const extension = name.includes(".") ? name.split(".").pop() ?? "" : "";
  if (["ts", "tsx", "mts", "cts"].includes(extension)) return "typescript";
  if (["js", "jsx", "mjs", "cjs"].includes(extension)) return "javascript";
  if (extension === "rs") return "rust";
  if (["py", "pyi", "pyw"].includes(extension)) return "python";
  if (["html", "htm", "css", "scss", "sass", "less", "vue", "svelte"].includes(extension)) return "web";
  if (["json", "jsonc", "json5"].includes(extension)) return "json";
  if (["md", "mdx", "markdown"].includes(extension)) return "markdown";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "avif"].includes(extension)) return "image";
  if (["zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "tar"].includes(extension)) return "archive";
  if (["sql", "db", "sqlite", "sqlite3"].includes(extension)) return "database";
  if (["toml", "yaml", "yml", "ini", "env", "conf", "config", "xml"].includes(extension)) return "config";
  if (["txt", "log", "csv"].includes(extension)) return "text";
  return "file";
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
    setState((previous) => {
      const reusePreviousResults = previous?.ownerKey === browser.ownerKey && previous.paths.length > 0;
      return {
        activeIndex: 0,
        loading: !reusePreviousResults,
        ownerKey: browser.ownerKey,
        paths: reusePreviousResults ? previous.paths : [],
        token,
      };
    });
    const timer = globalThis.setTimeout(() => {
      void browser.searchFiles(token.query).then((result) => {
        if (!current) return;
        setState({
          activeIndex: 0,
          error: result.state === "unavailable"
            ? result.notice ?? "Workspace files are unavailable."
            : undefined,
          loading: false,
          ownerKey: browser.ownerKey,
          paths: result.paths,
          token,
        });
      }, (error: unknown) => {
        if (!current) return;
        setState((previous) => ({
          activeIndex: 0,
          error: previous?.paths.length
            ? undefined
            : error instanceof Error ? error.message : "Workspace files are unavailable.",
          loading: false,
          ownerKey: browser.ownerKey,
          paths: previous?.ownerKey === browser.ownerKey ? previous.paths : [],
          token,
        }));
      });
    }, 40);
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
          <FileKindIcon path={path} />
          <span>{path}</span>
        </button>
      ))}
    </div>
  );
}

function FileKindIcon({ path }: { path: string }) {
  const kind = fileIconKind(path);
  const props = { "aria-hidden": true, size: 14 } as const;
  const icon = kind === "archive" ? <FileArchive {...props} />
    : kind === "config" ? <FileCog {...props} />
      : kind === "database" ? <Database {...props} />
        : kind === "image" ? <FileImage {...props} />
          : kind === "json" ? <Braces {...props} />
            : kind === "markdown" || kind === "text" ? <FileText {...props} />
              : ["javascript", "python", "rust", "typescript", "web"].includes(kind)
                ? <Code2 {...props} />
                : <File {...props} />;
  return <span className="composer-file-kind-icon" data-file-kind={kind}>{icon}</span>;
}
