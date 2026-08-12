import { spawn } from "node:child_process";
import { access, mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_EXTENSION_FILES = [
  // VSCE normalizes the conventional changelog asset to lowercase in VSIX archives.
  "changelog.md",
  "LICENSE.txt",
  "dist/extension.js",
  "webview/dist/assets/index.js",
  "webview/dist/assets/index.css",
  "webview/dist/mermaid-renderer.html",
  "webview/dist/mermaid-renderer.js",
];

/** Verifies packaged content and exercises the bundled native App Server lifecycle. */
export async function smokeReleaseVsix({ extensionRoot, version, target, binaryName }) {
  const manifest = JSON.parse(await readFile(path.join(extensionRoot, "package.json"), "utf8"));
  if (manifest.name !== "openaide-vscode-extension" || manifest.publisher !== "openaide") {
    throw new Error("VSIX package identity must be openaide.openaide-vscode-extension");
  }
  if (manifest.version !== version) {
    throw new Error(`VSIX package version ${manifest.version} does not match ${version}`);
  }
  for (const relativePath of REQUIRED_EXTENSION_FILES) {
    await access(path.join(extensionRoot, relativePath));
  }

  const appServerDirectory = path.join(extensionRoot, "dist", "app-server");
  const entries = (await readdir(appServerDirectory)).toSorted();
  if (entries.length !== 1 || entries[0] !== binaryName) {
    throw new Error(`VSIX must contain only ${binaryName} in dist/app-server; found: ${entries.join(", ")}`);
  }
  const binaryPath = path.join(appServerDirectory, binaryName);
  if (target !== "win32-x64") {
    const mode = (await stat(binaryPath)).mode;
    if ((mode & 0o111) === 0) {
      throw new Error(`Bundled App Server is not executable for ${target}`);
    }
  }

  const vsixManifest = await readFile(path.join(path.dirname(extensionRoot), "extension.vsixmanifest"), "utf8");
  const escapedTarget = target.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (!new RegExp(`TargetPlatform[^>]+${escapedTarget}|${escapedTarget}[^>]+TargetPlatform`).test(vsixManifest)) {
    throw new Error(`VSIX manifest does not declare target ${target}`);
  }
  const declaresPreRelease = /<Property\b(?=[^>]*\bId=["']Microsoft\.VisualStudio\.Code\.PreRelease["'])(?=[^>]*\bValue=["']true["'])[^>]*\/>/i
    .test(vsixManifest);
  const versionIsPreRelease = version.includes("-");
  if (declaresPreRelease !== versionIsPreRelease) {
    throw new Error(
      versionIsPreRelease
        ? `VSIX ${version} must declare native prerelease metadata`
        : `Stable VSIX ${version} must not declare prerelease metadata`,
    );
  }

  await smokeAppServer(binaryPath);
}

async function smokeAppServer(binaryPath) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-release-smoke-"));
  try {
    const child = spawn(binaryPath, [], {
      env: {
        ...process.env,
        OPENAIDE_APP_SERVER_PROTOCOL: "shell-control-stdio",
        OPENAIDE_STORAGE_ROOT: path.join(root, "state"),
      },
      stdio: "pipe",
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk) => { stdout += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
    child.stdin.end(`${JSON.stringify({
      jsonrpc: "2.0",
      id: "release-smoke-shutdown",
      method: "runtime.shutdown",
      params: {},
    })}\n`);

    const result = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        child.kill();
        reject(new Error("Bundled App Server did not shut down within 15 seconds"));
      }, 15_000);
      child.once("error", (error) => {
        clearTimeout(timeout);
        reject(error);
      });
      // `close` fires after stdio has drained; `exit` can race the final JSON-RPC response.
      child.once("close", (code, signal) => {
        clearTimeout(timeout);
        resolve({ code, signal });
      });
    });
    const shutdownResponse = stdout
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => {
        try { return JSON.parse(line); } catch { return undefined; }
      })
      .find((message) => message?.id === "release-smoke-shutdown");
    if (result.code !== 0 || !shutdownResponse || shutdownResponse.error) {
      throw new Error(
        `Bundled App Server smoke failed (code=${result.code}, signal=${result.signal}, stdout=${stdout.slice(0, 500)}): ${stderr.slice(0, 1_000)}`,
      );
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function main() {
  const [extensionRoot, version, target, binaryName] = process.argv.slice(2);
  if (!extensionRoot || !version || !target || !binaryName) {
    throw new Error("Usage: node scripts/smoke-release-vsix.mjs <extension-root> <version> <target> <binary>");
  }
  await smokeReleaseVsix({ extensionRoot: path.resolve(extensionRoot), version, target, binaryName });
  console.log(`Verified packaged ${target} VSIX and bundled App Server startup.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
