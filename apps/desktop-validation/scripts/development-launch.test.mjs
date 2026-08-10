import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import { runDesktopDevelopment } from "./development-launch.mjs";

test("F5 builds and starts Desktop with isolated development storage", async () => {
  const repositoryRoot = path.resolve(path.sep, "work", "OpenAIDE");
  const calls = [];

  await runDesktopDevelopment({
    repositoryRoot,
    platform: "win32",
    run(command, args, options) {
      calls.push({ command, args, cwd: options.cwd, env: options.env });
    },
  });

  assert.deepEqual(
    calls.map(({ command, args, cwd }) => ({ command, args, cwd })),
    [
      { command: "npm.cmd", args: ["install"], cwd: repositoryRoot },
      {
        command: "cargo.exe",
        args: ["build", "-p", "openaide-app-server"],
        cwd: repositoryRoot,
      },
      {
        command: "npm.cmd",
        args: [
          "run",
          "tauri",
          "--workspace",
          "openaide-desktop-validation",
          "--",
          "dev",
          "--config",
          "src-tauri/tauri.development.conf.json",
        ],
        cwd: repositoryRoot,
      },
    ],
  );

  const launchEnvironment = calls.at(-1).env;
  assert.equal(
    launchEnvironment.OPENAIDE_STORAGE_ROOT,
    path.join(repositoryRoot, ".vscode-test", "desktop", "state"),
  );
  assert.equal(
    launchEnvironment.OPENAIDE_RUNTIME_ROOT,
    path.join(repositoryRoot, ".vscode-test", "desktop", "runtime"),
  );
  assert.equal(
    launchEnvironment.OPENAIDE_DESKTOP_CREDENTIAL_NAMESPACE,
    "dev.openaide.desktop-development",
  );
  assert.equal(
    launchEnvironment.OPENAIDE_APP_SERVER_BIN,
    path.join(repositoryRoot, "target", "debug", "openaide-app-server.exe"),
  );
});

test("macOS F5 launches the source build through an addressable app bundle", async () => {
  const repositoryRoot = path.resolve(path.sep, "work", "OpenAIDE");
  const calls = [];

  await runDesktopDevelopment({
    repositoryRoot,
    platform: "darwin",
    run(command, args, options) {
      calls.push({ command, args, cwd: options.cwd });
    },
  });

  assert.deepEqual(calls.at(-1), {
    command: "npm",
    args: [
      "run",
      "tauri",
      "--workspace",
      "openaide-desktop-validation",
      "--",
      "dev",
      "--config",
      "src-tauri/tauri.development.conf.json",
      "--config",
      "src-tauri/tauri.development.macos.conf.json",
      "--runner",
      "../scripts/macos-development-runner.mjs",
    ],
    cwd: repositoryRoot,
  });
});
