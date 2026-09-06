import { readFile, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { describe, expect, it, vi } from "vitest";
import { TerminalHostManager } from "./hostTerminal";

const workspace = vi.hoisted(() => ({
  workspaceFolders: [] as { uri: { fsPath: string } }[],
  getConfiguration: () => ({ get: () => undefined }),
}));
vi.mock("vscode", () => ({ workspace }));

// /proc distinguishes a terminated orphan awaiting init's reap from a process
// that can still execute work. These tests use real processes and inherited pipes.
describe.skipIf(process.platform !== "linux")("terminal process ownership", () => {
  it("settles spawn failure without exposing Node's negative sentinel as an ACP exit code", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-terminal-spawn-"));
    workspace.workspaceFolders = [{ uri: { fsPath: root } }];
    const manager = new TerminalHostManager();
    try {
      const created = await manager.create({
        sessionId: "session_1", command: path.join(root, "missing-command"), cwd: root,
      });
      const ref = { sessionId: "session_1", terminalId: created.terminalId };
      await expect(manager.waitForExit(ref)).resolves.toEqual({ exitCode: null, signal: null });
      expect(manager.output(ref).output).toContain("ENOENT");
    } finally {
      manager.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it.each(["kill", "dispose"] as const)("%s stops a descendant after the command exits", async (action) => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-terminal-process-"));
    workspace.workspaceFolders = [{ uri: { fsPath: root } }];
    const manager = new TerminalHostManager();
    let descendantPid: number | undefined;
    try {
      const childCode = `process.on('SIGTERM', () => {}); console.log('child-ready'); setInterval(() => {}, 1000);`;
      const created = await manager.create({
        sessionId: "session_1", command: process.execPath, cwd: root,
        args: ["-e", `const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(childCode)}], { stdio: 'inherit' }); console.log(JSON.stringify({ parentPid: process.pid, descendantPid: child.pid })); setInterval(() => {}, 1000);`],
      });
      const ref = { sessionId: "session_1", terminalId: created.terminalId };
      await until(() => manager.output(ref).output.includes("child-ready"));
      descendantPid = JSON.parse(manager.output(ref).output.split("\n")[0]).descendantPid;

      if (action === "kill") manager.kill(ref);
      else manager.dispose();

      await until(async () => !(await isRunning(descendantPid!)));
      if (action === "kill") {
        await manager.waitForExit(ref);
        expect(manager.output(ref)).toHaveProperty("exitStatus");
      }
    } finally {
      manager.dispose();
      stopFixture(descendantPid);
      await rm(root, { recursive: true, force: true });
    }
  });

  it("waits for inherited stdout to finish before publishing exit and decoding final output", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-terminal-output-"));
    const releasePath = path.join(root, "release-output");
    workspace.workspaceFolders = [{ uri: { fsPath: root } }];
    const manager = new TerminalHostManager();
    let descendantPid: number | undefined;
    try {
      const childCode = `const timer = setInterval(() => { if (require('node:fs').existsSync(${JSON.stringify(releasePath)})) { clearInterval(timer); process.stdout.write(Buffer.from([0x82, 0xac])); } }, 10);`;
      const created = await manager.create({
        sessionId: "session_1", command: process.execPath, cwd: root,
        args: ["-e", `const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(childCode)}], { stdio: 'inherit' }); console.log(JSON.stringify({ parentPid: process.pid, descendantPid: child.pid })); process.stdout.write(Buffer.from([0xe2])); process.exit(0);`],
      });
      const ref = { sessionId: "session_1", terminalId: created.terminalId };
      let settled = false;
      const completion = Promise.resolve(manager.waitForExit(ref)).then((status) => {
        settled = true;
        return status;
      });
      await until(() => manager.output(ref).output.includes("\n"));
      const pids = JSON.parse(manager.output(ref).output.split("\n")[0]);
      descendantPid = pids.descendantPid;
      await until(async () => !(await isRunning(pids.parentPid)));

      expect(settled, "command exit must not release waiters before inherited output closes").toBe(false);
      await writeFile(releasePath, "continue");
      await expect(completion).resolves.toEqual({ exitCode: 0, signal: null });
      expect(manager.output(ref).output).toBe(`${JSON.stringify(pids)}\n€`);
    } finally {
      manager.dispose();
      stopFixture(descendantPid);
      await rm(root, { recursive: true, force: true });
    }
  });

  it("retires background descendants that closed inherited pipes before command completion", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-terminal-background-"));
    workspace.workspaceFolders = [{ uri: { fsPath: root } }];
    const manager = new TerminalHostManager();
    let descendantPid: number | undefined;
    try {
      const childCode = `process.on('SIGTERM', () => {}); process.send('ready'); setInterval(() => {}, 1000);`;
      const created = await manager.create({
        sessionId: "session_1", command: process.execPath, cwd: root,
        args: ["-e", `const child = require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(childCode)}], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'] }); child.once('message', () => { console.log(child.pid); child.disconnect(); process.exit(0); });`],
      });
      const ref = { sessionId: "session_1", terminalId: created.terminalId };
      await manager.waitForExit(ref);
      descendantPid = Number(manager.output(ref).output.trim());
      expect(descendantPid).toBeGreaterThan(0);
      await until(async () => !(await isRunning(descendantPid!)));
    } finally {
      manager.dispose();
      stopFixture(descendantPid);
      await rm(root, { recursive: true, force: true });
    }
  });
});

async function until(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 3_500;
  while (!(await check())) {
    if (Date.now() >= deadline) throw new Error("Timed out waiting for terminal process lifecycle");
    await delay(20);
  }
}

async function isRunning(pid: number) {
  try {
    const status = await readFile(`/proc/${pid}/status`, "utf8");
    return !/^State:\s+Z/m.test(status);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
}

function stopFixture(pid: number | undefined) {
  if (!pid) return;
  try { process.kill(pid, "SIGKILL"); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; }
}
