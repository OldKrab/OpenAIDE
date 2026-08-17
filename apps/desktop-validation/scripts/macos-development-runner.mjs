#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { pathToFileURL } from "node:url";

const DEVELOPMENT_CONFIG = "src-tauri/tauri.development.conf.json";
const DEVELOPMENT_MACOS_CONFIG = "src-tauri/tauri.development.macos.conf.json";
const DEVELOPMENT_PRODUCT_NAME = "OpenAIDE Development";
const BINARY_NAME = "openaide-desktop-validation";

/** Converts the command Tauri sends to its runner into an equivalent build. */
export function cargoBuildArguments(runnerArguments) {
  const separator = runnerArguments.indexOf("--");
  const cargoArguments = runnerArguments.slice(0, separator === -1 ? undefined : separator);
  if (cargoArguments[0] !== "run") {
    throw new Error(`Expected Tauri to invoke the development runner with cargo run, received: ${cargoArguments.join(" ")}`);
  }
  cargoArguments[0] = "build";
  return cargoArguments;
}

export function applicationArguments(runnerArguments) {
  const separator = runnerArguments.indexOf("--");
  return separator === -1 ? [] : runnerArguments.slice(separator + 1);
}

export function developmentBundleExecutable(srcTauriRoot, environment) {
  const targetRoot = environment.CARGO_TARGET_DIR
    ? path.resolve(srcTauriRoot, environment.CARGO_TARGET_DIR)
    : path.join(srcTauriRoot, "target");
  return path.join(
    targetRoot,
    "debug",
    "bundle",
    "macos",
    `${DEVELOPMENT_PRODUCT_NAME}.app`,
    "Contents",
    "MacOS",
    BINARY_NAME,
  );
}

/** Builds, bundles, and runs the macOS F5 binary while preserving Tauri's watch lifecycle. */
export async function runMacOsDevelopmentBundle({
  environment = process.env,
  runnerArguments = process.argv.slice(2),
  srcTauriRoot = process.cwd(),
} = {}) {
  const cargo = environment.CARGO ?? "cargo";
  const build = spawnSync(cargo, cargoBuildArguments(runnerArguments), {
    cwd: srcTauriRoot,
    env: environment,
    stdio: "inherit",
  });
  if (build.error) throw build.error;
  if (build.status !== 0) return build.status ?? 1;

  const appRoot = path.dirname(srcTauriRoot);
  const bundle = spawnSync(
    "npm",
    [
      "run",
      "tauri",
      "--",
      "bundle",
      "--debug",
      "--bundles",
      "app",
      "--config",
      DEVELOPMENT_CONFIG,
      "--config",
      DEVELOPMENT_MACOS_CONFIG,
      "--no-sign",
    ],
    { cwd: appRoot, env: environment, stdio: "inherit" },
  );
  if (bundle.error) throw bundle.error;
  if (bundle.status !== 0) return bundle.status ?? 1;

  const application = spawn(
    developmentBundleExecutable(srcTauriRoot, environment),
    applicationArguments(runnerArguments),
    {
      cwd: srcTauriRoot,
      env: {
        ...environment,
        OPENAIDE_DEVELOPMENT_RUNNER_PID: String(process.pid),
      },
      stdio: "inherit",
    },
  );
  const forwardSignal = (signal) => {
    if (!application.killed) application.kill(signal);
  };
  process.once("SIGINT", forwardSignal);
  process.once("SIGTERM", forwardSignal);
  return await new Promise((resolve, reject) => {
    application.once("error", reject);
    application.once("exit", (code, signal) => {
      process.removeListener("SIGINT", forwardSignal);
      process.removeListener("SIGTERM", forwardSignal);
      resolve(code ?? (signal ? 1 : 0));
    });
  });
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  process.exitCode = await runMacOsDevelopmentBundle();
}
