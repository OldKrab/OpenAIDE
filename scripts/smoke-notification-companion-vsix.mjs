import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Verifies the local companion package without invoking the host OS notifier. */
export async function smokeNotificationCompanionVsix({ extensionRoot, version }) {
  const manifest = await readJson(path.join(extensionRoot, "package.json"));
  if (manifest.name !== "openaide-vscode-notification-companion" || manifest.publisher !== "openaide") {
    throw new Error("Notification companion VSIX has the wrong package identity");
  }
  if (manifest.version !== version) {
    throw new Error(`Notification companion VSIX version ${manifest.version} does not match ${version}`);
  }
  for (const relativePath of ["dist/extension.js", "readme.md", "LICENSE.txt"]) {
    await readFile(path.join(extensionRoot, relativePath));
  }
}

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const [extensionRoot, version] = process.argv.slice(2);
  if (!extensionRoot || !version) throw new Error("Usage: node scripts/smoke-notification-companion-vsix.mjs <extensionRoot> <version>");
  await smokeNotificationCompanionVsix({ extensionRoot, version });
}
