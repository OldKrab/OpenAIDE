import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const EXTENSION_ID = "openaide.openaide-vscode-extension";
const TARGETS = ["linux-x64", "win32-x64", "darwin-arm64"];
const RELEASE_EXTENSIONS = [
  {
    extensionId: EXTENSION_ID,
    targets: TARGETS,
    fileName: (version, target) => `openaide-vscode-${target}-${version}.vsix`,
  },
];

/** Publishes missing registry packages and treats an identical existing package as success. */
export async function reconcileRegistry({
  registry,
  version,
  packages,
  fetchImpl = fetch,
  publish = publishPackage,
  wait = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
  attempts = 30,
}) {
  const lookup = registry === "marketplace" ? lookupMarketplaceDigest : registry === "open-vsx" ? lookupOpenVsxDigest : undefined;
  if (!lookup) throw new Error(`Unsupported release registry: ${registry}`);

  for (const releasePackage of packages) {
    const expected = await sha256(releasePackage.path);
    const facts = {
      fetchImpl,
      version,
      target: releasePackage.target,
      extensionId: releasePackage.extensionId,
    };
    const existing = await lookup(facts);
    if (existing === expected) {
      console.log(`${registry} ${version} ${releasePackage.target} already matches ${expected}; skipping.`);
      continue;
    }
    if (existing) {
      throw new Error(`${registry} ${version} ${releasePackage.target} has digest ${existing}, expected ${expected}`);
    }

    await publish({ registry, packagePath: releasePackage.path });
    let published;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      published = await lookup(facts);
      if (published) break;
      await wait(2_000);
    }
    if (published !== expected) {
      throw new Error(
        published
          ? `${registry} ${version} ${releasePackage.target} published digest ${published}, expected ${expected}`
          : `${registry} ${version} ${releasePackage.target} was not visible after publication`,
      );
    }
    console.log(`Published ${registry} ${version} ${releasePackage.target} with digest ${expected}.`);
  }
}

export async function lookupMarketplaceDigest({ fetchImpl, version, target, extensionId = EXTENSION_ID }) {
  const response = await fetchImpl(
    "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery?api-version=7.2-preview.1",
    {
      method: "POST",
      headers: { accept: "application/json", "content-type": "application/json" },
      body: JSON.stringify({
        filters: [{ criteria: [{ filterType: 7, value: extensionId }] }],
        // Include every version and its properties; latest-only would break recovery of older releases.
        flags: 17,
      }),
    },
  );
  if (!response.ok) throw new Error(`Marketplace lookup failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
  const body = await response.json();
  const extension = body.results?.[0]?.extensions?.find(
    (candidate) => `${candidate.publisher?.publisherName}.${candidate.extensionName}` === extensionId,
  );
  const release = extension?.versions?.find((candidate) => candidate.version === version && (
    target === "universal"
      ? !candidate.targetPlatform || candidate.targetPlatform === "universal"
      : candidate.targetPlatform === target
  ));
  const digest = release?.properties?.find(
    (property) => property.key === "Microsoft.VisualStudio.Services.VsixSha256",
  )?.value;
  return normalizeDigest(digest);
}

export async function lookupOpenVsxDigest({ fetchImpl, version, target, extensionId = EXTENSION_ID }) {
  const [publisher, extensionName] = extensionId.split(".", 2);
  const targetPath = target === "universal" ? version : `${target}/${version}`;
  const response = await fetchImpl(`https://open-vsx.org/api/${publisher}/${extensionName}/${targetPath}`);
  if (response.status === 404) return undefined;
  if (!response.ok) throw new Error(`Open VSX lookup failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
  const metadata = await response.json();
  if (!metadata.files?.sha256) throw new Error(`Open VSX ${version} ${target} did not expose a SHA-256 file`);
  const digestResponse = await fetchImpl(metadata.files.sha256);
  if (!digestResponse.ok) {
    throw new Error(`Open VSX digest lookup failed (${digestResponse.status}): ${(await digestResponse.text()).slice(0, 500)}`);
  }
  return normalizeDigest(await digestResponse.text());
}

export async function collectReleasePackages(directory, version) {
  const entries = await readdir(directory);
  return RELEASE_EXTENSIONS.flatMap((extension) => extension.targets.map((target) => {
    const name = extension.fileName(version, target);
    if (!entries.includes(name)) throw new Error(`Canonical GitHub release is missing ${name}`);
    return { extensionId: extension.extensionId, target, path: path.join(directory, name) };
  }));
}

async function sha256(filePath) {
  return createHash("sha256").update(await readFile(filePath)).digest("hex");
}

function normalizeDigest(value) {
  if (!value) return undefined;
  const digest = value.trim().toLowerCase().replace(/^sha256:/, "");
  if (!/^[0-9a-f]{64}$/.test(digest)) {
    throw new Error(`Registry returned an invalid SHA-256 digest: ${value}`);
  }
  return digest;
}

async function publishPackage({ registry, packagePath }) {
  const executable = process.platform === "win32" ? "npm.cmd" : "npm";
  const args = registry === "marketplace"
    ? ["exec", "--", "vsce", "publish", "--packagePath", packagePath]
    : ["exec", "--", "ovsx", "publish", packagePath];
  const result = spawnSync(executable, args, { encoding: "utf8", stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${registry} publisher exited with status ${result.status}`);
}

async function main() {
  const [registry, version, directory = "release"] = process.argv.slice(2);
  if (!registry || !version) {
    throw new Error("Usage: node scripts/reconcile-release-registry.mjs <marketplace|open-vsx> <version> [directory]");
  }
  await reconcileRegistry({
    registry,
    version,
    packages: await collectReleasePackages(path.resolve(directory), version),
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
