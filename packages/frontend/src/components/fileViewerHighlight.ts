import { common, createLowlight } from "lowlight";
import type { Element, Root, RootContent } from "hast";

/** App Server sends a basename extension; this maps it onto Highlight.js grammars. */
const LANGUAGE_BY_ALIAS: Record<string, string> = {
  bash: "bash",
  c: "c",
  cc: "cpp",
  cjs: "javascript",
  conf: "ini",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  diff: "diff",
  env: "ini",
  go: "go",
  graphql: "graphql",
  gql: "graphql",
  h: "c",
  hh: "cpp",
  hpp: "cpp",
  htm: "xml",
  html: "xml",
  hxx: "cpp",
  ini: "ini",
  java: "java",
  js: "javascript",
  json: "json",
  jsonc: "json",
  jsx: "javascript",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  m: "objectivec",
  md: "markdown",
  mk: "makefile",
  mjs: "javascript",
  mm: "objectivec",
  mts: "typescript",
  patch: "diff",
  php: "php",
  pl: "perl",
  pm: "perl",
  py: "python",
  pyi: "python",
  rb: "ruby",
  rs: "rust",
  scss: "scss",
  sh: "bash",
  sql: "sql",
  svg: "xml",
  swift: "swift",
  toml: "ini",
  ts: "typescript",
  tsx: "typescript",
  vb: "vbnet",
  vue: "xml",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zsh: "bash",
};

const lowlight = createLowlight(common);

export type FileViewerSpan = {
  className?: string;
  text: string;
};

export function highlightFileViewerLines(text: string, language?: string | null): FileViewerSpan[][] {
  const expected = text.split("\n").length;
  const grammar = resolveGrammar(language);
  if (!grammar) return plainLines(text);
  try {
    const lines = linesFromTree(lowlight.highlight(grammar, text));
    if (lines.length !== expected) return plainLines(text);
    return lines.map((spans) => (spans.length === 0 ? [{ text: " " }] : spans));
  } catch {
    return plainLines(text);
  }
}

function resolveGrammar(language?: string | null) {
  if (!language) return undefined;
  const alias = language.trim().replace(/^\./, "").toLowerCase();
  if (!alias) return undefined;
  const grammar = LANGUAGE_BY_ALIAS[alias] ?? alias;
  return lowlight.registered(grammar) ? grammar : undefined;
}

function plainLines(text: string): FileViewerSpan[][] {
  return text.split("\n").map((line) => (line === "" ? [{ text: " " }] : [{ text: line }]));
}

function linesFromTree(root: Root): FileViewerSpan[][] {
  const lines: FileViewerSpan[][] = [[]];
  visit(root.children, lines);
  return lines;
}

function visit(nodes: RootContent[], lines: FileViewerSpan[][], inherited?: string) {
  for (const node of nodes) {
    if (node.type === "text") {
      appendText(lines, node.value, inherited);
      continue;
    }
    if (node.type !== "element") continue;
    visit(node.children, lines, mergeClass(inherited, elementClass(node)));
  }
}

function appendText(lines: FileViewerSpan[][], value: string, className?: string) {
  const parts = value.split("\n");
  for (let index = 0; index < parts.length; index += 1) {
    if (index > 0) lines.push([]);
    const text = parts[index];
    if (!text) continue;
    const line = lines[lines.length - 1];
    const last = line[line.length - 1];
    if (last && last.className === className) {
      last.text += text;
      continue;
    }
    line.push(className ? { className, text } : { text });
  }
}

function elementClass(node: Element) {
  const value = node.properties?.className;
  if (Array.isArray(value)) return value.map(String).join(" ");
  return typeof value === "string" ? value : "";
}

function mergeClass(inherited: string | undefined, next: string) {
  return [inherited, next].filter(Boolean).join(" ") || undefined;
}
