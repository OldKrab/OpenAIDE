import { appendFile, readFile, writeFile } from "node:fs/promises";
import process from "node:process";
import { fileURLToPath } from "node:url";

const packageName = "@agentclientprotocol/codex-acp";
const configPath =
  process.env.CODEX_ACP_CONFIG_PATH ??
  fileURLToPath(
    new URL("../openaide-rs/app-server/src/agent/acp_agent_config.rs", import.meta.url),
  );
const packagePattern =
  /@agentclientprotocol\/codex-acp@(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)/g;

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
const latest = requireExactVersion(await publishedVersion(), "npm registry");
const changed = current !== latest;

if (changed) {
  const updated = source.replace(packagePattern, `${packageName}@${latest}`);
  await writeFile(configPath, updated);
  console.log(`Updated ${packageName} from ${current} to ${latest}`);
} else {
  console.log(`${packageName} is already current at ${current}`);
}

await emitOutput("changed", changed);
await emitOutput("current", current);
await emitOutput("latest", latest);
