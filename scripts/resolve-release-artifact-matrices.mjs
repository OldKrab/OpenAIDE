import { fileURLToPath } from "node:url";

const servers = {
  "linux-x64": { os: "ubuntu-24.04", target: "linux-x64", binary: "openaide-app-server" },
  "win32-x64": { os: "windows-2025", target: "win32-x64", binary: "openaide-app-server.exe" },
  "darwin-arm64": { os: "macos-15", target: "darwin-arm64", binary: "openaide-app-server" },
  "linux-x64-musl": {
    os: "ubuntu-24.04",
    target: "linux-x64-musl",
    binary: "openaide-app-server",
    rust_target: "x86_64-unknown-linux-musl",
  },
};

const desktops = {
  "win32-x64": {
    os: "windows-2025",
    target: "win32-x64",
    target_triple: "x86_64-pc-windows-msvc",
    binary: "openaide-app-server.exe",
    sidecar_extension: ".exe",
    bundle: "nsis",
    bundle_directory: "nsis",
    extension: "exe",
  },
  "darwin-arm64": {
    os: "macos-15",
    target: "darwin-arm64",
    target_triple: "aarch64-apple-darwin",
    binary: "openaide-app-server",
    sidecar_extension: "",
    bundle: "dmg",
    bundle_directory: "dmg",
    extension: "dmg",
  },
};

const artifactSelections = ["all", "vsix", "desktop"];
const vsixTargets = ["linux-x64", "win32-x64", "darwin-arm64"];
const desktopTargets = ["win32-x64", "darwin-arm64"];

function selectTargets(selection, supported, inputName) {
  if (selection === "all") return supported;
  if (supported.includes(selection)) return [selection];
  throw new Error(`${inputName} must be all, ${supported.slice(0, -1).join(", ")}, or ${supported.at(-1)}`);
}

function uniqueServers(rows) {
  return [...new Map(rows.map((row) => [row.target, row])).values()];
}

export function resolveReleaseArtifactMatrices({ artifacts, vsixTarget, desktopTarget }) {
  if (!artifactSelections.includes(artifacts)) {
    throw new Error("artifacts must be all, vsix, or desktop");
  }

  const selectedVsixTargets = selectTargets(vsixTarget, vsixTargets, "vsix_target");
  const selectedDesktopTargets = selectTargets(desktopTarget, desktopTargets, "desktop_target");
  const vsixServers = selectedVsixTargets.map((target) => servers[target]);
  const desktopServers = selectedDesktopTargets.flatMap((target) =>
    target === "win32-x64"
      ? [servers["win32-x64"], servers["linux-x64-musl"]]
      : [servers[target]],
  );

  const requiredServers = artifacts === "vsix"
    ? vsixServers
    : artifacts === "desktop"
      ? desktopServers
      // VSIX and desktop can share native server artifacts.
      : uniqueServers([...vsixServers, ...desktopServers]);

  return {
    desktop_matrix: { include: selectedDesktopTargets.map((target) => desktops[target]) },
    server_matrix: { include: requiredServers },
    vsix_matrix: { include: vsixServers },
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [artifacts, vsixTarget, desktopTarget] = process.argv.slice(2);
  try {
    const matrices = resolveReleaseArtifactMatrices({ artifacts, vsixTarget, desktopTarget });
    for (const [name, matrix] of Object.entries(matrices)) {
      process.stdout.write(`${name}=${JSON.stringify(matrix)}\n`);
    }
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
