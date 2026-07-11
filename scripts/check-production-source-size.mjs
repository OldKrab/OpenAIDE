import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MAX_LOGICAL_LINES = 400;
const PRODUCTION_WORKSPACE_ROOTS = new Set(["apps", "openaide-rs", "packages"]);
const SOURCE_EXTENSIONS = new Set([
  ".css",
  ".cjs",
  ".html",
  ".js",
  ".jsx",
  ".mjs",
  ".rs",
  ".scss",
  ".sh",
  ".ts",
  ".tsx",
]);
const EXCLUDED_DIRECTORIES = new Set([
  ".agents",
  ".workflow",
  "__tests__",
  "dist",
  "fixtures",
  "generated",
  "node_modules",
  "target",
  "test",
  "tests",
  "vendor",
  "vendored",
]);

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const files = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  { cwd: repoRoot, encoding: "utf8" },
).split("\0").filter(Boolean);

const productionSources = files
  .filter(isProductionSource)
  .filter((file) => existsSync(path.join(repoRoot, file)))
  .map((file) => ({ file, source: readFileSync(path.join(repoRoot, file), "utf8") }));
const oversized = productionSources
  .map(({ file, source }) => ({ file, lines: logicalLineCount(source) }))
  .filter(({ lines }) => lines > MAX_LOGICAL_LINES)
  .sort((left, right) => right.lines - left.lines || left.file.localeCompare(right.file));
const inlineRustTests = productionSources
  .filter(({ file, source }) => file.endsWith(".rs") && /#\[cfg\(test\)\]\s*mod\s+\w+\s*\{/.test(source))
  .map(({ file }) => file)
  .sort();

if (oversized.length > 0 || inlineRustTests.length > 0) {
  const details = oversized.map(({ file, lines }) => `  ${lines} ${file}`).join("\n");
  const inlineTestDetails = inlineRustTests.map((file) => `  ${file}`).join("\n");
  throw new Error(
    [
      oversized.length > 0
        ? `Hand-written production source must stay at or below ${MAX_LOGICAL_LINES} logical lines:\n${details}`
        : undefined,
      inlineRustTests.length > 0
        ? `Rust test bodies must live in separate test files:\n${inlineTestDetails}`
        : undefined,
    ].filter(Boolean).join("\n"),
  );
}

console.log(`Production source policy check passed (${MAX_LOGICAL_LINES} logical lines maximum).`);

function isProductionSource(file) {
  const normalized = file.split(path.sep).join("/");
  const segments = normalized.split("/");
  const basename = segments.at(-1) ?? "";
  // Workspace runtime code lives under <workspace>/<package>/src. Repository tooling,
  // deployment scripts, examples, and package configuration follow separate policies.
  if (!PRODUCTION_WORKSPACE_ROOTS.has(segments[0]) || segments[2] !== "src") return false;
  if (!SOURCE_EXTENSIONS.has(path.extname(basename))) return false;
  if (segments.some((segment) => EXCLUDED_DIRECTORIES.has(segment))) return false;
  if (/\.(?:spec|test)\.[^.]+$/.test(basename)) return false;
  return basename !== "test.rs" && basename !== "tests.rs";
}

function logicalLineCount(source) {
  return source.split(/\r?\n/).filter((line) => line.trim().length > 0).length;
}
