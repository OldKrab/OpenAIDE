import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** Builds the current workspace and starts the isolated Desktop validation shell. */
export async function runDesktopDevelopment({
  repositoryRoot,
  platform = process.platform,
  run = runCommand,
}) {
  const executableSuffix = platform === "win32" ? ".exe" : "";
  const commandSuffix = platform === "win32" ? ".cmd" : "";
  const environment = {
    ...process.env,
    OPENAIDE_STORAGE_ROOT: path.join(repositoryRoot, ".vscode-test", "desktop", "state"),
    OPENAIDE_RUNTIME_ROOT: path.join(repositoryRoot, ".vscode-test", "desktop", "runtime"),
    OPENAIDE_DESKTOP_CREDENTIAL_NAMESPACE: "dev.openaide.desktop-development",
    OPENAIDE_APP_SERVER_BIN: path.join(
      repositoryRoot,
      "target",
      "debug",
      `openaide-app-server${executableSuffix}`,
    ),
  };
  const tauriArguments = [
    "run",
    "tauri",
    "--workspace",
    "openaide-desktop-validation",
    "--",
    "dev",
    "--config",
    "src-tauri/tauri.development.conf.json",
  ];
  if (platform === "darwin") {
    // A bare `cargo run` process has no macOS application bundle, so
    // accessibility clients cannot address the F5 app independently from an
    // installed build. The runner packages the debug binary before launching it.
    tauriArguments.push(
      "--config",
      "src-tauri/tauri.development.macos.conf.json",
      "--runner",
      "../scripts/macos-development-runner.mjs",
    );
  }

  await run(`npm${commandSuffix}`, ["install"], { cwd: repositoryRoot, env: environment });
  await run(`cargo${executableSuffix}`, ["build", "-p", "openaide-app-server"], {
    cwd: repositoryRoot,
    env: environment,
  });
  await run(
    `npm${commandSuffix}`,
    tauriArguments,
    { cwd: repositoryRoot, env: environment },
  );
}

function runCommand(command, args, options) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status ?? "unknown"}`);
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
  await runDesktopDevelopment({ repositoryRoot });
}
