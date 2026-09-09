import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { StringDecoder } from "node:string_decoder";
import * as vscode from "vscode";
import { ExtensionLogger } from "../logging/logger";
import type { RuntimeClient } from "./rpcClient";
import { terminalEnvironment } from "./hostTerminalEnvironment";
import { appendDecoderRemainder, appendOutput } from "./hostTerminalOutput";
import { parseCreateTerminalParams, parseTerminalIdParams } from "./hostTerminalParams";
import { signalTerminalProcess } from "./hostTerminalProcess";
import type { TerminalExitStatus, TerminalRecord } from "./hostTerminalTypes";
import {
  TERMINAL_CREATE,
  TERMINAL_KILL,
  TERMINAL_OUTPUT,
  TERMINAL_RELEASE,
  TERMINAL_WAIT_FOR_EXIT,
} from "./hostTerminalTypes";

export function registerTerminalHostHandlers(runtime: RuntimeClient): vscode.Disposable {
  const manager = new TerminalHostManager();
  const create = runtime.onHostRequest(TERMINAL_CREATE, (params) => manager.create(params));
  const output = runtime.onHostRequest(TERMINAL_OUTPUT, (params) => manager.output(params));
  const wait = runtime.onHostRequest(TERMINAL_WAIT_FOR_EXIT, (params) => manager.waitForExit(params));
  const kill = runtime.onHostRequest(TERMINAL_KILL, (params) => manager.kill(params));
  const release = runtime.onHostRequest(TERMINAL_RELEASE, (params) => manager.release(params));

  return {
    dispose: () => {
      create.dispose();
      output.dispose();
      wait.dispose();
      kill.dispose();
      release.dispose();
      manager.dispose();
    },
  };
}

export class TerminalHostManager implements vscode.Disposable {
  private readonly terminals = new Map<string, TerminalRecord>();
  private disposed = false;

  constructor(private readonly logger = new ExtensionLogger("terminal-host")) {}

  async create(params: unknown) {
    if (this.disposed) throw new Error("Terminal host is disposed");
    const request = await parseCreateTerminalParams(params);
    // Filesystem validation may outlive the shell owner. No process may be
    // admitted after dispose has drained the terminals it owns.
    if (this.disposed) throw new Error("Terminal host is disposed");
    const terminalId = `term_${randomUUID()}`;
    const startedAt = Date.now();
    this.logger.info("terminal_process_started", { terminal_id: terminalId, session_id: request.sessionId });
    const child = spawn(request.command, request.args, {
      cwd: request.cwd,
      env: terminalEnvironment(request.env),
      shell: false,
      stdio: "pipe",
      windowsHide: true,
      // POSIX commands own a process group so Stop also reaches subprocesses.
      // Windows detached processes have different console/lifetime semantics.
      detached: process.platform !== "win32",
    });

    const record: TerminalRecord = {
      id: terminalId,
      startedAt,
      sessionId: request.sessionId,
      child,
      stdoutDecoder: new StringDecoder("utf8"),
      stderrDecoder: new StringDecoder("utf8"),
      output: "",
      truncated: false,
      outputByteLimit: request.outputByteLimit,
      exitStatus: undefined,
      released: false,
      forceKillTimer: undefined,
      treeKillProcess: undefined,
      waiters: [],
    };
    this.terminals.set(terminalId, record);

    child.stdout.on("data", (chunk: Buffer) => appendOutput(record, record.stdoutDecoder, chunk));
    child.stderr.on("data", (chunk: Buffer) => appendOutput(record, record.stderrDecoder, chunk));
    child.once("error", (error) => {
      appendOutput(record, record.stderrDecoder, Buffer.from(`${error.message}\n`, "utf8"));
      this.logger.warn("terminal_process_error", { terminal_id: record.id, error_code: (error as NodeJS.ErrnoException).code });
    });
    // `exit` only reaps the direct child; descendants may still hold its pipes.
    // Keep output decoders, waiters, and Stop escalation alive until EOF.
    child.once("close", (code, signal) => {
      if (process.platform !== "win32" && record.child.pid !== undefined) {
        // A background descendant can close inherited pipes without exiting.
        // Retire the remaining group now, before publishing terminal completion.
        this.cleanup(record, "SIGKILL");
      }
      this.finish(record, {
        // Node uses negative sentinels when spawn fails; ACP accepts only real
        // unsigned process exit codes, or null when no process exited.
        exitCode: code !== null && code < 0 ? null : code,
        signal: signal ?? null,
      });
    });

    return { terminalId };
  }

  output(params: unknown) {
    const record = this.requireTerminal(params);
    return {
      output: record.output,
      truncated: record.truncated,
      ...(record.exitStatus ? { exitStatus: record.exitStatus } : {}),
    };
  }

  waitForExit(params: unknown) {
    const record = this.requireTerminal(params);
    if (record.exitStatus) return record.exitStatus;
    return new Promise<TerminalExitStatus>((resolve) => {
      record.waiters.push(resolve);
    });
  }

  kill(params: unknown) {
    const record = this.requireTerminal(params);
    this.killRecord(record);
    this.scheduleForceKill(record);
    return {};
  }

  release(params: unknown) {
    const record = this.requireTerminal(params);
    record.released = true;
    if (!record.exitStatus) {
      this.killRecord(record);
      this.scheduleForceKill(record);
      return {};
    }
    this.terminals.delete(record.id);
    return {};
  }

  dispose() {
    this.disposed = true;
    for (const record of this.terminals.values()) {
      if (!record.exitStatus) {
        this.cleanup(record, "SIGKILL");
      }
      if (record.forceKillTimer) {
        clearTimeout(record.forceKillTimer);
      }
    }
    this.terminals.clear();
  }

  private requireTerminal(params: unknown) {
    const { sessionId, terminalId } = parseTerminalIdParams(params);
    const record = this.terminals.get(terminalId);
    if (!record || record.released) {
      throw new Error("terminal not found");
    }
    if (record.sessionId !== sessionId) {
      throw new Error("terminal not found");
    }
    return record;
  }

  private killRecord(record: TerminalRecord, signal?: NodeJS.Signals) {
    if (record.exitStatus) return;
    signalTerminalProcess(record, signal, this.logger);
  }

  private cleanup(record: TerminalRecord, signal: NodeJS.Signals) {
    try {
      this.killRecord(record, signal);
    } catch (error) {
      this.logger.warn("terminal_process_cleanup_failed", {
        terminal_id: record.id, signal, error_code: (error as NodeJS.ErrnoException).code,
      });
    }
  }

  private scheduleForceKill(record: TerminalRecord) {
    if (record.forceKillTimer) return;
    record.forceKillTimer = setTimeout(() => {
      record.forceKillTimer = undefined;
      if (!record.exitStatus) {
        this.cleanup(record, "SIGKILL");
      }
    }, 2_000);
    record.forceKillTimer.unref?.();
  }

  private finish(record: TerminalRecord, status: TerminalExitStatus) {
    if (record.exitStatus) return;
    appendDecoderRemainder(record);
    if (record.forceKillTimer) {
      clearTimeout(record.forceKillTimer);
      record.forceKillTimer = undefined;
    }
    record.exitStatus = status;
    this.logger.info("terminal_process_finished", {
      terminal_id: record.id, session_id: record.sessionId, duration_ms: Date.now() - record.startedAt,
      exit_code: status.exitCode, signal: status.signal,
      outcome: status.signal !== null ? "signaled" : status.exitCode === 0 ? "completed" : "failed",
    });
    for (const waiter of record.waiters.splice(0)) {
      waiter(status);
    }
    if (record.released) {
      this.terminals.delete(record.id);
    }
  }
}
