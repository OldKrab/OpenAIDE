import { cpSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";

const source = resolve("packages/frontend/dist");
const target = resolve("apps/vscode-extension/webview/dist");

rmSync(target, { recursive: true, force: true });
mkdirSync(resolve("apps/vscode-extension/webview"), { recursive: true });
cpSync(source, target, { recursive: true });
