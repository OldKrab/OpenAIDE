import { mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const extensionRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const distRoot = path.join(extensionRoot, "dist");

mkdirSync(distRoot, { recursive: true });
for (const entry of readdirSync(distRoot, { withFileTypes: true })) {
  // The platform App Server is staged independently and must survive extension rebundles.
  if (entry.name !== "app-server") {
    rmSync(path.join(distRoot, entry.name), { recursive: true, force: true });
  }
}
