import { appendFile, readFile, writeFile } from "node:fs/promises";
import process from "node:process";
import { fileURLToPath } from "node:url";

const packageName = "@openaide/claude-agent-acp";
// The launch policy shared by App Server and the catalog owns the pin.
const policyPath = process.env.CLAUDE_ACP_MANIFEST_PATH ?? fileURLToPath(
  new URL("../packages/app-shell-contracts/src/agentCatalog/claude-acp/package.json", import.meta.url),
);

function requireExactVersion(value, source) {
  if (
    typeof value !== "string" ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(value)
  ) {
    throw new Error(`${source} did not provide an exact semantic version`);
  }
  return value;
}

// A requested version comes from the adapter's publish workflow; a scheduled
// run follows the `latest` dist-tag. Either must already resolve on npm,
// because the built-in launches this exact version through npx.
async function publishedVersion(requested) {
  const override = process.env.CLAUDE_ACP_PUBLISHED_VERSION;
  if (override) {
    return override;
  }

  const response = await fetch(
    `https://registry.npmjs.org/${encodeURIComponent(packageName)}/${requested || "latest"}`,
  );
  if (!response.ok) {
    throw new Error(`npm registry returned ${response.status} ${response.statusText}`);
  }

  const metadata = await response.json();
  return metadata.version;
}

async function emitOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) {
    await appendFile(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
  }
}

const requested = process.env.CLAUDE_ACP_TARGET_VERSION
  ? requireExactVersion(process.env.CLAUDE_ACP_TARGET_VERSION, "requested version")
  : "";
const policy = JSON.parse(await readFile(policyPath, "utf8"));
const current = requireExactVersion(policy.dependencies?.[packageName], "launch policy");
const latest = requireExactVersion(await publishedVersion(requested), "npm registry");
if (requested && latest !== requested) {
  throw new Error(`npm registry resolved ${latest} instead of the requested ${requested}`);
}
const changed = current !== latest;

if (changed) {
  policy.dependencies[packageName] = latest;
  await writeFile(policyPath, `${JSON.stringify(policy, null, 2)}\n`);
  console.log(`${packageName} pin updated from ${current} to ${latest}`);
} else {
  console.log(`${packageName} is already current at ${current}`);
}

await emitOutput("changed", changed);
await emitOutput("current", current);
await emitOutput("latest", latest);
