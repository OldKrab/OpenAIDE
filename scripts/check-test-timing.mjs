import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { isTestSource, timingViolations } from "./test-timing-policy.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const files = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  { cwd: repoRoot, encoding: "utf8" },
).split("\0").filter(Boolean);

const violations = files
  .filter(isTestSource)
  .filter((file) => existsSync(path.join(repoRoot, file)))
  .flatMap((file) => timingViolations(file, readFileSync(path.join(repoRoot, file), "utf8"))
    .map((violation) => `  ${file}:${violation.line} ${violation.reason}: ${violation.text}`));

if (violations.length > 0) {
  throw new Error(
    `Tests must order themselves with gates instead of time. Follow "Time in tests" in docs/testing.md:\n${violations.join("\n")}`,
  );
}
