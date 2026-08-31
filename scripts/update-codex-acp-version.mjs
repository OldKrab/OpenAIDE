import { appendFile, readFile, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import process from "node:process";
import { fileURLToPath } from "node:url";

const packageName = "@openaide/codex-acp";
const execute = promisify(execFile);
const configPath =
  process.env.CODEX_ACP_CONFIG_PATH ??
  fileURLToPath(
    new URL("../openaide-rs/app-server/src/agent/acp_agent_config.rs", import.meta.url),
  );
const packagePattern =
  /@openaide\/codex-acp@(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)/g;
const provisionerPath = fileURLToPath(
  new URL("../openaide-rs/app-server/src/agent/codex_acp_provisioner.rs", import.meta.url),
);
const runtimeManifestPath = fileURLToPath(
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

const source = await readFile(configPath, "utf8");
const matches = [...source.matchAll(packagePattern)];
if (matches.length !== 1) {
  throw new Error(`expected one ${packageName} pin in ${configPath}, found ${matches.length}`);
}

const current = matches[0][1];
const provisionerSource = await readFile(provisionerPath, "utf8");
const provisionerVersion = provisionerSource.match(/CODEX_ACP_VERSION: &str = "([^"]+)"/)?.[1];
const runtimeManifest = JSON.parse(await readFile(runtimeManifestPath, "utf8"));
const runtimeLock = JSON.parse(await readFile(
  fileURLToPath(new URL("../openaide-rs/app-server/assets/codex-acp-runtime/package-lock.json", import.meta.url)),
  "utf8",
));
if (
  provisionerVersion !== current ||
  runtimeManifest.dependencies?.[packageName] !== current ||
  runtimeLock.packages?.[""]?.dependencies?.[packageName] !== current ||
  runtimeLock.packages?.[`node_modules/${packageName}`]?.version !== current
) {
  throw new Error("Codex ACP policy, runtime manifest, and lockfile versions must match");
}
const latest = requireExactVersion(await publishedVersion(), "npm registry");
const changed = current !== latest;

if (changed) {
  const updated = source.replace(packagePattern, `${packageName}@${latest}`);
  await writeFile(configPath, updated);
  const updatedProvisioner = provisionerSource.replace(
    /CODEX_ACP_VERSION: &str = "[^"]+"/,
    `CODEX_ACP_VERSION: &str = "${latest}"`,
  );
  if (updatedProvisioner === provisionerSource) {
    throw new Error(`expected a Codex ACP version constant in ${provisionerPath}`);
  }
  await writeFile(provisionerPath, updatedProvisioner);

  runtimeManifest.dependencies[packageName] = latest;
  await writeFile(runtimeManifestPath, `${JSON.stringify(runtimeManifest, null, 2)}\n`);
  await execute(
    process.platform === "win32" ? "npm.cmd" : "npm",
    ["install", "--package-lock-only", "--ignore-scripts", "--no-audit", "--no-fund"],
    { cwd: fileURLToPath(new URL("../openaide-rs/app-server/assets/codex-acp-runtime", import.meta.url)) },
  );
  console.log(`Updated ${packageName} from ${current} to ${latest}`);
} else {
  console.log(`${packageName} is already current at ${current}`);
}

await emitOutput("changed", changed);
await emitOutput("current", current);
await emitOutput("latest", latest);
