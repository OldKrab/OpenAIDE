import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const outputPath = path.join(root, "packages/app-server-client/src/generated/protocol.ts");
const output = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "openaide-app-server-protocol", "--bin", "export_ts"],
  {
    cwd: root,
    encoding: "utf8",
  },
);

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, output);
