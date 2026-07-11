import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { StringDecoder } from "node:string_decoder";
import * as vscode from "vscode";
import type { RuntimeClient } from "./rpcClient";
import { terminalEnvironment } from "./hostTerminalEnvironment";
import { appendDecoderRemainder, appendOutput } from "./hostTerminalOutput";
import { parseCreateTerminalParams, parseTerminalIdParams } from "./hostTerminalParams";
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

  async create(params: unknown) {
    const request = await parseCreateTerminalParams(params);
    const terminalId = `term_${randomUUID()}`;
    const child = spawn(request.command, request.args, {
      cwd: request.cwd,
      env: terminalEnvironment(request.env),
      shell: false,
      stdio: "pipe",
      windowsHide: true,
    });

    const record: TerminalRecord = {
      id: terminalId,
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
      waiters: [],
    };
    this.terminals.set(terminalId, record);

    child.stdout.on("data", (chunk: Buffer) => appendOutput(record, record.stdoutDecoder, chunk));
    child.stderr.on("data", (chunk: Buffer) => appendOutput(record, record.stderrDecoder, chunk));
    child.once("error", (error) => {
      appendOutput(record, record.stderrDecoder, Buffer.from(`${error.message}\n`, "utf8"));
      this.finish(record, { exitCode: null, signal: null });
    });
    child.once("exit", (code, signal) => {
      this.finish(record, {
        exitCode: code,
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
    for (const record of this.terminals.values()) {
      if (!record.exitStatus) {
        this.killRecord(record, "SIGKILL");
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
    record.child.kill(signal);
  }

  private scheduleForceKill(record: TerminalRecord) {
    if (record.forceKillTimer) return;
    record.forceKillTimer = setTimeout(() => {
      record.forceKillTimer = undefined;
      if (!record.exitStatus) {
        this.killRecord(record, "SIGKILL");
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
    for (const waiter of record.waiters.splice(0)) {
      waiter(status);
    }
    if (record.released) {
      this.terminals.delete(record.id);
    }
  }
}
