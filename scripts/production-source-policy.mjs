import path from "node:path";

const PRODUCTION_WORKSPACE_ROOTS = new Set(["apps", "openaide-rs", "packages"]);
const SOURCE_EXTENSIONS = new Set([
  ".css", ".cjs", ".html", ".js", ".jsx", ".mjs", ".rs", ".scss", ".sh", ".ts", ".tsx",
]);
const EXCLUDED_DIRECTORIES = new Set([
  ".agents", ".workflow", "__tests__", "dist", "fixtures", "generated", "node_modules",
  "target", "test", "tests", "vendor", "vendored",
]);

/** Returns whether a tracked file is hand-written production source. */
export function isProductionSource(file) {
  const normalized = file.split(path.sep).join("/");
  const segments = normalized.split("/");
  const basename = segments.at(-1) ?? "";
  if (!PRODUCTION_WORKSPACE_ROOTS.has(segments[0]) || segments[2] !== "src") return false;
  if (!SOURCE_EXTENSIONS.has(path.extname(basename))) return false;
  if (segments.some((segment) => EXCLUDED_DIRECTORIES.has(segment))) return false;
  if (/\.(?:spec|test)\.[^.]+$/.test(basename)) return false;
  if (basename.endsWith("_tests.rs")) return false;
  return basename !== "test.rs" && basename !== "tests.rs";
}

export function logicalLineCount(source) {
  return source.split(/\r?\n/).filter((line) => line.trim().length > 0).length;
}
