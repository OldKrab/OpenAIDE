import { execFile } from "node:child_process";
import { win32 } from "node:path";
import type { ExtensionLogger } from "../logging/logger";
import type { TerminalRecord } from "./hostTerminalTypes";

/** Stop the command's owned process group/tree without shell command parsing. */
export function signalTerminalProcess(record: TerminalRecord, signal: NodeJS.Signals | undefined, logger: Pick<ExtensionLogger, "warn">) {
  const child = record.child;
  if (process.platform !== "win32") {
    if (child.pid === undefined) {
      child.kill(signal);
      return;
    }
    try {
      process.kill(-child.pid, signal ?? "SIGTERM");
    } catch (error) {
      // A naturally completed group may disappear before Stop/close runs.
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
    return;
  }

  if (child.pid === undefined || child.exitCode !== null || child.signalCode !== null) {
    // Windows cannot rediscover a process tree through an already-exited PID.
    // Full lifetime ownership would require a native Job Object at spawn time.
    child.kill(signal);
    return;
  }
  if (record.treeKillProcess) return;
  // Use the OS utility by absolute path, never a terminal-controlled PATH/cwd.
  // /F matches Node's existing forceful Windows child.kill behavior; /T extends
  // it to active descendants. The bounded helper outlives synchronous dispose.
  const executable = win32.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "taskkill.exe");
  record.treeKillProcess = execFile(executable, ["/PID", String(child.pid), "/T", "/F"], {
    windowsHide: true,
    timeout: 2_000,
  }, (error) => {
    record.treeKillProcess = undefined;
    if (!error) return;
    logger.warn("terminal_process_tree_cleanup_failed", { terminal_id: record.id, error_code: error.code });
    // Preserve the old direct-child cleanup if taskkill is missing or denied.
    // ChildProcess retains its native handle, avoiding a stale PID fallback.
    child.kill(signal);
  });
}
