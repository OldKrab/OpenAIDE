import { appendFile, readFile } from "node:fs/promises";
import process from "node:process";
import { fileURLToPath } from "node:url";

const packageName = "@openaide/codex-acp";
const recoveryManifestPath = fileURLToPath(
  new URL("../openaide-rs/app-server/assets/codex-acp-runtime/session-recovery-manifest.json", import.meta.url),
);
const runtimeManifestPath = process.env.CODEX_ACP_MANIFEST_PATH ?? fileURLToPath(
  new URL("../openaide-rs/app-server/assets/codex-acp-runtime/package.json", import.meta.url),
);

async function publishedVersion() {
  const override = process.env.CODEX_ACP_LATEST_VERSION;
  if (override) {
    return override;
  }

  const response = await fetch(
    `https://registry.npmjs.org/${encodeURIComponent(packageName)}/latest`,
  );
  if (!response.ok) {
    throw new Error(`npm registry returned ${response.status} ${response.statusText}`);
  }

  const metadata = await response.json();
  return metadata.version;
}

function requireExactVersion(value, source) {
  if (
    typeof value !== "string" ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(value)
  ) {
    throw new Error(`${source} did not provide an exact semantic version`);
  }
  return value;
}

async function emitOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) {
    await appendFile(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
  }
}

const recoveryManifest = JSON.parse(await readFile(recoveryManifestPath, "utf8"));
const runtimeManifest = JSON.parse(await readFile(runtimeManifestPath, "utf8"));
const current = requireExactVersion(runtimeManifest.dependencies?.[packageName], "runtime manifest");
const runtimeLock = JSON.parse(await readFile(
  fileURLToPath(new URL("../openaide-rs/app-server/assets/codex-acp-runtime/package-lock.json", import.meta.url)),
  "utf8",
));
if (
  recoveryManifest.packageName !== packageName ||
  recoveryManifest.packageVersion !== current ||
  runtimeManifest.dependencies?.[packageName] !== current ||
  runtimeLock.packages?.[""]?.dependencies?.[packageName] !== current ||
  runtimeLock.packages?.[`node_modules/${packageName}`]?.version !== current
) {
  throw new Error("Codex ACP policy, runtime manifest, and lockfile versions must match");
}
const latest = requireExactVersion(await publishedVersion(), "npm registry");
const changed = current !== latest;

if (changed) {
  // TODO(codex-acp-1.2.0): restore automatic upgrades after removing the
  // byte-pinned recovery patch. A newer npm version cannot inherit its hashes.
  throw new Error(
    `${packageName}@${latest} requires reviewing or removing the managed session recovery patch before updating pins`,
  );
} else {
  console.log(`${packageName} is already current at ${current}`);
}

await emitOutput("changed", changed);
await emitOutput("current", current);
await emitOutput("latest", latest);
