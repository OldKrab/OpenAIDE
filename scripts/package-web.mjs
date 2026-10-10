#!/usr/bin/env node
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Stages the packaged Web App: the production server, the built Frontend, the
 * App Server executable, and the service files. The layout mirrors the
 * repository so `apps/web/src/server.mjs` resolves the same siblings in a
 * checkout and in an install.
 *
 * Callers build the Frontend, the TypeScript packages, and the App Server first.
 * TODO: apps/android/scripts/package-runtime.mjs stages the same three pieces
 * for Termux; move it onto this function once Android launches server.mjs.
 */
export function stageWebPackage({ appServerPath, output, root = repoRoot, version }) {
  const frontendDist = path.join(root, "packages/frontend/dist");
  for (const required of [appServerPath, path.join(frontendDist, "index.html")]) {
    if (!existsSync(required)) throw new Error(`Web package input is missing: ${required}`);
  }
  rmSync(output, { recursive: true, force: true });
  mkdirSync(output, { recursive: true });

  cpSync(path.join(root, "apps/web/src"), path.join(output, "apps/web/src"), {
    recursive: true,
    filter: (source) => !/[.-]test(?:-support)?\.mjs$/.test(source),
  });
  // Source maps are the bulk of the build and nothing in the app loads them.
  cpSync(frontendDist, path.join(output, "packages/frontend/dist"), {
    recursive: true,
    filter: (source) => !source.endsWith(".map"),
  });
  for (const name of ["app-server-client", "app-shell-contracts"]) {
    const source = path.join(root, "packages", name);
    const destination = path.join(output, "node_modules/@openaide", name);
    if (!existsSync(path.join(source, "dist"))) throw new Error(`Web package input is missing: ${source}/dist`);
    mkdirSync(destination, { recursive: true });
    copyFileSync(path.join(source, "package.json"), path.join(destination, "package.json"));
    cpSync(path.join(source, "dist"), path.join(destination, "dist"), { recursive: true });
  }

  mkdirSync(path.join(output, "bin"), { recursive: true });
  for (const [source, name] of [
    [appServerPath, "openaide-app-server"],
    [path.join(root, "deploy/web/openaide-web"), "openaide-web"],
  ]) {
    copyFileSync(source, path.join(output, "bin", name));
    chmodSync(path.join(output, "bin", name), 0o755);
  }
  copyFileSync(path.join(root, "deploy/web/openaide-web.service"), path.join(output, "openaide-web.service"));
  copyFileSync(path.join(root, "deploy/web/README.md"), path.join(output, "README.md"));
  copyFileSync(path.join(root, "LICENSE"), path.join(output, "LICENSE"));
  // The server entry is an ES module and needs this marker outside a workspace.
  writeFileSync(
    path.join(output, "package.json"),
    `${JSON.stringify({ name: "openaide-web", version, license: "AGPL-3.0-only", private: true, type: "module" }, null, 2)}\n`,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [appServerPath, output, version] = process.argv.slice(2);
  if (!appServerPath || !output) {
    console.error("Usage: node scripts/package-web.mjs <app-server-executable> <output-directory> [version]");
    process.exit(2);
  }
  stageWebPackage({
    appServerPath: path.resolve(appServerPath),
    output: path.resolve(output),
    version: version ?? JSON.parse(readFileSync(path.join(repoRoot, "package.json"), "utf8")).version,
  });
  console.log(`Staged the Web App package in ${path.resolve(output)}`);
}
