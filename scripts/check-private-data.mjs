import path from "node:path";
import { fileURLToPath } from "node:url";

import { privateDataViolations } from "./private-data-policy.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const violations = privateDataViolations(repoRoot);

if (violations.length > 0) {
  const report = violations.map(({ file, line, rule }) => `  ${file}:${line} ${rule}`).join("\n");
  console.error(
    `Staged content names a personal host or a private term. Move the value to an ignored local file, or use example.com and a generic fixture user:\n${report}`,
  );
  process.exit(1);
}
