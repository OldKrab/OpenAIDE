import { fileURLToPath } from "node:url";

const artifactSelections = ["all", "vsix", "desktop"];
const vsixTargets = ["linux-x64", "win32-x64", "darwin-arm64"];
const desktopTargets = ["win32-x64", "darwin-arm64"];

const dependencies = Object.freeze({
  "vsix-linux-x64": ["server-linux-x64-musl"],
  "vsix-win32-x64": ["server-win32-x64"],
  "vsix-darwin-arm64": ["server-darwin-arm64"],
  "desktop-win32-x64": ["server-win32-x64", "server-linux-x64-musl"],
  "desktop-darwin-arm64": ["server-darwin-arm64"],
});

const nodeOrder = [
  "server-linux-x64-musl",
  "server-win32-x64",
  "server-darwin-arm64",
  ...Object.keys(dependencies),
];

function selectedTargets(selection, supported, inputName) {
  if (selection === "all") return supported;
  if (supported.includes(selection)) return [selection];
  throw new Error(
    `${inputName} must be all, ${supported.slice(0, -1).join(", ")}, or ${supported.at(-1)}`,
  );
}

/** Owns release-node selection and the exact App Server dependencies of every package. */
export function resolveReleaseArtifactGraph({ artifacts, vsixTarget, desktopTarget }) {
  if (!artifactSelections.includes(artifacts)) {
    throw new Error("artifacts must be all, vsix, or desktop");
  }
  const selectedVsixTargets = selectedTargets(vsixTarget, vsixTargets, "vsix_target");
  const selectedDesktopTargets = selectedTargets(
    desktopTarget,
    desktopTargets,
    "desktop_target",
  );
  const enabledPackages = new Set();
  if (artifacts !== "desktop") {
    for (const target of selectedVsixTargets) {
      enabledPackages.add(`vsix-${target}`);
    }
  }
  if (artifacts !== "vsix") {
    for (const target of selectedDesktopTargets) {
      enabledPackages.add(`desktop-${target}`);
    }
  }

  const enabledServers = new Set(
    [...enabledPackages].flatMap((node) => dependencies[node] ?? []),
  );
  return {
    dependencies,
    enabledNodes: nodeOrder.filter((node) => enabledServers.has(node) || enabledPackages.has(node)),
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [artifacts, vsixTarget, desktopTarget] = process.argv.slice(2);
  try {
    const graph = resolveReleaseArtifactGraph({ artifacts, vsixTarget, desktopTarget });
    const enabled = new Set(graph.enabledNodes);
    for (const node of nodeOrder) {
      process.stdout.write(`${node.replaceAll("-", "_")}=${enabled.has(node)}\n`);
    }
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
