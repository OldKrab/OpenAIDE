import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { isProductionSource, logicalLineCount } from "./production-source-policy.mjs";

const MAX_LOGICAL_LINES = 800;
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
