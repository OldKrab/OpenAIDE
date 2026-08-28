import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import semver from "semver";

const platforms = new Set(["windows-x86_64", "darwin-aarch64"]);
const channels = new Set(["stable", "prerelease"]);
const signingIdentities = new Set(["release", "recovery"]);
const artifactPrefix = "https://github.com/OldKrab/OpenAIDE/releases/download/";

export function buildDesktopUpdateFeed(graph, { repositoryRoot = process.cwd() } = {}) {
  if (graph?.schemaVersion !== 1) fail("schemaVersion must be 1");
  if (graph.repository !== "OldKrab/OpenAIDE") fail("repository must be OldKrab/OpenAIDE");
  if (!Array.isArray(graph.edges)) fail("edges must be an array");

  const bySource = new Map();
  for (const [index, edge] of graph.edges.entries()) {
    validateEdge(edge, index, repositoryRoot);
    const key = `${edge.channel}:${edge.source}:${edge.platform}`;
    if (bySource.has(key)) fail(`duplicate update edge ${key}`);
    bySource.set(key, edge);
  }
  validatePlatformPairs(graph.edges);
  validateReachability(graph.edges);

  return new Map(graph.edges.map((edge) => [manifestPath(edge), manifest(edge)]));
}

export function writeDesktopUpdateFeed(feed, outputDirectory) {
  const parent = path.dirname(outputDirectory);
  const temporary = path.join(
    parent,
    `.${path.basename(outputDirectory)}-${process.pid}-${Date.now()}`,
  );
  fs.rmSync(temporary, { force: true, recursive: true });
  for (const [relativePath, value] of feed) {
    const destination = path.join(temporary, relativePath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, `${JSON.stringify(value, null, 2)}\n`);
  }
  fs.mkdirSync(parent, { recursive: true });
  fs.rmSync(outputDirectory, { force: true, recursive: true });
  fs.renameSync(temporary, outputDirectory);
}

function validateEdge(edge, index, repositoryRoot) {
  const label = `edges[${index}]`;
  if (!semver.valid(edge.source) || !semver.valid(edge.target)) {
    fail(`${label} source and target must be exact SemVer`);
  }
  if (!semver.gt(edge.target, edge.source)) fail(`${label} target must increase SemVer`);
  if (!channels.has(edge.channel)) fail(`${label} has unsupported channel`);
  if (!platforms.has(edge.platform)) fail(`${label} has unsupported platform`);
  if (!signingIdentities.has(edge.signingIdentity)) fail(`${label} has unknown signing identity`);
  if (typeof edge.compatibilityEvidence !== "string" || !edge.compatibilityEvidence) {
    fail(`${label} requires compatibility evidence`);
  }
  const evidence = path.resolve(repositoryRoot, edge.compatibilityEvidence);
  if (!fs.existsSync(evidence)) fail(`${label} compatibility evidence does not exist`);
  if (typeof edge.notes !== "string" || Buffer.byteLength(edge.notes) > 64 * 1024) {
    fail(`${label} notes must be bounded text`);
  }
  if (edge.publishedAt !== undefined && Number.isNaN(Date.parse(edge.publishedAt))) {
    fail(`${label} publishedAt must be RFC 3339`);
  }
  if (typeof edge.artifact?.url !== "string" || !edge.artifact.url.startsWith(artifactPrefix)) {
    fail(`${label} artifact must be an immutable canonical GitHub Release URL`);
  }
  const releasePath = edge.artifact.url.slice(artifactPrefix.length);
  if (!releasePath.startsWith(`v${edge.target}/`) || releasePath.includes("/latest/")) {
    fail(`${label} artifact URL must identify the target release tag`);
  }
  if (!/^[a-f0-9]{64}$/.test(edge.artifact.sha256 ?? "")) {
    fail(`${label} artifact sha256 must be lowercase hexadecimal`);
  }
  if (!Number.isSafeInteger(edge.artifact.size) || edge.artifact.size <= 0) {
    fail(`${label} artifact size must be a positive integer`);
  }
  if (typeof edge.artifact.signature !== "string" || edge.artifact.signature.length < 32) {
    fail(`${label} artifact signature is missing`);
  }
}

function validatePlatformPairs(edges) {
  const groups = Map.groupBy(edges, (edge) => `${edge.channel}:${edge.source}`);
  for (const [key, group] of groups) {
    const found = new Set(group.map((edge) => edge.platform));
    if (found.size !== platforms.size || [...platforms].some((platform) => !found.has(platform))) {
      fail(`${key} must advance Windows and macOS together`);
    }
    if (new Set(group.map((edge) => edge.target)).size !== 1) {
      fail(`${key} must offer the same target on both platforms`);
    }
  }
}

function validateReachability(edges) {
  const groups = Map.groupBy(edges, (edge) => `${edge.channel}:${edge.platform}`);
  for (const [key, group] of groups) {
    const next = new Map(group.map((edge) => [edge.source, edge.target]));
    for (const source of next.keys()) {
      const visited = new Set();
      let version = source;
      while (next.has(version)) {
        if (visited.has(version)) fail(`${key} contains an update cycle`);
        visited.add(version);
        version = next.get(version);
      }
    }
  }
}

function manifestPath(edge) {
  const [target, arch] = edge.platform.split("-");
  return path.join("v1", edge.channel, target, arch, `${edge.source}.json`);
}

function manifest(edge) {
  return {
    version: edge.target,
    notes: edge.notes,
    ...(edge.publishedAt ? { pub_date: edge.publishedAt } : {}),
    url: edge.artifact.url,
    signature: edge.artifact.signature,
    signingIdentity: edge.signingIdentity,
    artifactSize: edge.artifact.size,
    artifactSha256: edge.artifact.sha256,
  };
}

function fail(message) {
  throw new Error(`Desktop Update Graph: ${message}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [graphPath = "release/desktop-update-graph.json", outputDirectory] = process.argv.slice(2);
  try {
    const graph = JSON.parse(fs.readFileSync(graphPath, "utf8"));
    const feed = buildDesktopUpdateFeed(graph);
    if (outputDirectory) writeDesktopUpdateFeed(feed, outputDirectory);
    process.stdout.write(`Validated ${graph.edges.length} Desktop Update edges.\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
