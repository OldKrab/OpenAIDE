import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/**
 * Builds everything the smoke stacks serve, once per run. Builds belong here
 * because global setup has no deadline: a cold compile inside a spec hook would
 * race that hook's timeout, and parallel workers would rebuild the same outputs.
 * Workers inherit the static roots through the environment.
 */
export default async function globalSetup() {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-smoke-build-"));
  const web = path.join(root, "web");
  const desktop = path.join(root, "desktop");
  try {
    await step("typescript_deps", "npm", ["run", "build:typescript-deps"]);
    await step("frontend_web", "npm", ["run", "build", "--workspace", "openaide-frontend", "--", "--outDir", web, "--emptyOutDir"]);
    await step("frontend_desktop", "npm", [
      "exec",
      "--workspace",
      "openaide-frontend",
      "vite",
      "--",
      "build",
      "--config",
      path.join(repoRoot, "tests/smoke/desktop-shell/vite.config.mjs"),
      "--outDir",
      desktop,
      "--emptyOutDir",
    ]);
    await step("app_server", "cargo", ["build", "-p", "openaide-app-server"]);
  } catch (error) {
    await rm(root, { recursive: true, force: true });
    throw error;
  }
  process.env.OPENAIDE_SMOKE_STATIC_ROOT_WEB = web;
  process.env.OPENAIDE_SMOKE_STATIC_ROOT_DESKTOP = desktop;
  return async () => {
    await rm(root, { recursive: true, force: true });
  };
}

async function step(name, command, args) {
  const started = Date.now();
  const child = spawn(command, args, {
    cwd: repoRoot,
    // A smoke build must never inherit Driver/Target roots or presentation.
    env: Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("OPENAIDE_"))),
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  child.stdout.on("data", (chunk) => { output += chunk; });
  child.stderr.on("data", (chunk) => { output += chunk; });
  const code = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", resolve);
  });
  const event = { operation: "smoke_build", step: name, outcome: code === 0 ? "ok" : "failed", duration_ms: Date.now() - started, exit: code };
  console.log(JSON.stringify(event));
  if (code !== 0) throw new Error(`${command} ${args.join(" ")} failed (${code}):\n${output}`);
}
