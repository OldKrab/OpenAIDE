import path from "node:path";

/** The shared watchdog every suite uses; a smaller test budget is a private clock. */
export const WATCHDOG_MS = 30_000;

/** Kinds a `timing:` marker may name. `docs/testing.md` defines each one. */
export const TIMING_KINDS = ["absence", "contract", "data", "expiry", "mocked", "poll"];

const TEST_EXTENSIONS = new Set([".cjs", ".js", ".mjs", ".rs", ".ts", ".tsx"]);
const TEST_DIRECTORIES = new Set(["__tests__", "test", "tests"]);
const SKIPPED_DIRECTORIES = new Set(["dist", "generated", "node_modules", "target", "vendor", "vendored"]);
// The one place a suite defines its durations; everything else names them.
const VOCABULARY_FILES = new Set(["test_sync.rs"]);

const MARKER = new RegExp(`timing: (?:${TIMING_KINDS.join("|")})\\b`);
const FILE_MARKER = new RegExp(`timing-file: (?:${TIMING_KINDS.join("|")}) — \\S`);
// A line that names the shared watchdog only bounds a hang.
const WATCHDOG_NAME = /\bWATCHDOG(?:_MS)?\b/;

const RUST_RULES = [
  [/\bsleep(?:_until)?\(/, "sleeps"],
  [/Duration::from_(?:secs|millis|micros|nanos)(?:_f(?:32|64))?\(/, "sizes a duration"],
  [/\.elapsed\(\)(?:\.\w+\(\))?\s*[<>]/, "asserts elapsed time"],
];
const SCRIPT_RULES = [
  [/\bwaitForTimeout\(|\bawait (?:delay|sleep)\(/, "sleeps"],
  [/(?<![.\w])setTimeout\((?!.*,\s*0\s*\))/, "sleeps"],
  [/\b(?:Date|performance)\.now\(\)\s*-\s*[\w.]+\s*[<>]/, "asserts elapsed time"],
];
// Literal sizes of a test budget, a deadline, or a timer.
const BUDGET = /(?:\btimeout(?:Ms)?:\s*|\.setTimeout\(\s*|\.now\(\)\s*\+\s*|\bsetTimeout\(.*,\s*(?=\d[\d_]*\s*\)))(\d[\d_]*)/g;

/** Returns whether a tracked file holds test code this policy covers. */
export function isTestSource(file) {
  const segments = file.split(path.sep).join("/").split("/");
  const basename = segments.at(-1) ?? "";
  if (!TEST_EXTENSIONS.has(path.extname(basename)) || VOCABULARY_FILES.has(basename)) return false;
  if (segments.some((segment) => SKIPPED_DIRECTORIES.has(segment))) return false;
  if (/\.(?:spec|test)\.[^.]+$/.test(basename) || /(?:^|_)tests?\.rs$/.test(basename)) return true;
  return segments.slice(0, -1).some((segment) => TEST_DIRECTORIES.has(segment));
}

/**
 * Finds every line where a test depends on real time without saying why.
 * A `timing: <kind> — reason` comment on the line or the line above accounts
 * for it; `timing-file:` in the file header accounts for the whole file.
 */
export function timingViolations(file, source) {
  const lines = source.split(/\r?\n/);
  if (lines.slice(0, 10).some((line) => FILE_MARKER.test(line))) return [];
  const rules = file.endsWith(".rs") ? RUST_RULES : SCRIPT_RULES;
  const violations = [];
  lines.forEach((line, index) => {
    const code = line.trim();
    if (/^(?:\/\/|\*|\/\*|#\s)/.test(code)) return;
    if (MARKER.test(line) || MARKER.test(lines[index - 1] ?? "") || WATCHDOG_NAME.test(line)) return;
    const reasons = rules.filter(([pattern]) => pattern.test(line)).map(([, reason]) => reason);
    if (!file.endsWith(".rs")) {
      const sizes = [...line.matchAll(BUDGET)].map((match) => Number(match[1].replaceAll("_", "")));
      if (sizes.some((size) => size >= WATCHDOG_MS)) return;
      if (sizes.some((size) => size > 0)) reasons.push("sizes a duration below the watchdog");
    }
    if (reasons.length > 0) violations.push({ line: index + 1, reason: [...new Set(reasons)].join(", "), text: code });
  });
  return violations;
}
