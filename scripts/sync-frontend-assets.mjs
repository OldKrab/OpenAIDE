import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function syncFrontendAssets(source, target) {
  rmSync(target, { recursive: true, force: true });
  mkdirSync(target, { recursive: true });
  cpSync(source, target, { recursive: true });

  // Vite's root-relative asset URLs suit web routes, while VS Code webviews
  // require resources to resolve beside the copied stylesheet.
  const stylesheet = resolve(target, "assets/index.css");
  const css = readFileSync(stylesheet, "utf8");
  writeFileSync(stylesheet, css.replace(/url\((['"]?)\/assets\//g, "url($1./"));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  syncFrontendAssets(
    resolve("packages/frontend/dist"),
    resolve("apps/vscode-extension/webview/dist"),
  );
}
