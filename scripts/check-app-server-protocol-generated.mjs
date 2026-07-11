import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const outputPath = path.join(root, "packages/app-server-client/src/generated/protocol.ts");
const expected = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "openaide-app-server-protocol", "--bin", "export_ts"],
  {
    cwd: root,
    encoding: "utf8",
  },
);
const actual = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8") : "";

if (actual !== expected) {
  console.error("Generated App Server Protocol TypeScript bindings are stale.");
  console.error("Run `npm run protocol:generate` and commit the result.");
  process.exit(1);
}

console.log("app server protocol generated bindings check passed");
