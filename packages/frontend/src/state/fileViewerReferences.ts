const FILE_EXTENSIONS = new Set([
  "c", "cc", "cpp", "cs", "css", "env", "ex", "exs", "go", "h", "hpp", "html", "java", "js", "json",
  "jsonc", "jsx", "kt", "lock", "lua", "md", "markdown", "mjs", "php", "proto", "py", "rb", "rs",
  "scss", "sh", "sql", "svg", "swift", "toml", "ts", "tsx", "txt", "xml", "yaml", "yml", "zig",
]);

export type AgentFileLocation = { path: string; line?: number };

export function markdownFileLocation(href: string | undefined): AgentFileLocation | undefined {
  if (!href) return undefined;
  let decoded: string;
  try {
    decoded = decodeURIComponent(href);
  } catch {
    return undefined;
  }
  const { path, line } = splitLineSuffix(decoded);
  return isAbsoluteFilePath(path) ? { path, line } : undefined;
}

export function pathLikeFileLocation(value: string): AgentFileLocation | undefined {
  const trimmed = value.trim();
  if (!trimmed || /\s/.test(trimmed)) return undefined;
  const { path, line } = splitLineSuffix(trimmed);
  if (!path || path === "." || path === "..") return undefined;
  if (isAbsoluteFilePath(path)) return { path, line };
  if (path.includes("/") || path.includes("\\")) {
    const basename = path.split(/[/\\]/).pop() ?? "";
    return hasKnownFileExtension(basename) ? { path, line } : undefined;
  }
  return hasKnownFileExtension(path) ? { path, line } : undefined;
}

export function relativeMarkdownHref(href: string | undefined): string | undefined {
  if (!href || markdownFileLocation(href)) return undefined;
  if (/^[a-z][a-z0-9+-]*:/i.test(href)) return undefined;
  return href;
}

/** Chat markdown file links: absolute paths or workspace-relative hrefs, not URLs or fragments. */
export function chatMarkdownFileLocation(href: string | undefined): AgentFileLocation | undefined {
  const absolute = markdownFileLocation(href);
  if (absolute) return absolute;
  const relative = relativeMarkdownHref(href);
  if (!relative || relative.startsWith("#")) return undefined;
  return pathLikeFileLocation(relative);
}

function splitLineSuffix(value: string): { path: string; line?: number } {
  const match = value.match(/^(.*):([1-9]\d*)$/);
  if (match?.[1]) return { path: match[1], line: Number(match[2]) };
  return { path: value };
}

function hasKnownFileExtension(fileName: string) {
  const dot = fileName.lastIndexOf(".");
  if (dot <= 0 || dot === fileName.length - 1) return false;
  return FILE_EXTENSIONS.has(fileName.slice(dot + 1).toLowerCase());
}

function isAbsoluteFilePath(value: string) {
  return value.startsWith("/") || /^[a-z]:[\\/]/i.test(value);
}
