import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";

const binaryName = process.platform === "win32" ? "openaide-app-server.exe" : "openaide-app-server";
const source = resolve("target", "debug", binaryName);
const targetDir = resolve("apps", "vscode-extension", "dist", "app-server");
const target = resolve(targetDir, binaryName);

if (!existsSync(source)) {
  throw new Error(`App Server binary not found at ${source}. Run cargo build -p openaide-app-server first.`);
}

rmSync(targetDir, { recursive: true, force: true });
mkdirSync(targetDir, { recursive: true });
copyFileSync(source, target);
if (process.platform !== "win32") {
  chmodSync(target, 0o755);
}
