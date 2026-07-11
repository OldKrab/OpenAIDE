import type { ChildProcessWithoutNullStreams } from "node:child_process";
import type { StringDecoder } from "node:string_decoder";

export const TERMINAL_CREATE = "terminal/create";
export const TERMINAL_OUTPUT = "terminal/output";
export const TERMINAL_WAIT_FOR_EXIT = "terminal/wait_for_exit";
export const TERMINAL_KILL = "terminal/kill";
export const TERMINAL_RELEASE = "terminal/release";

export type EnvVariable = {
  name: string;
  value: string;
};

export type CreateTerminalParams = {
  sessionId: string;
  command: string;
  args: string[];
  env: EnvVariable[];
  cwd: string;
  outputByteLimit: number;
};

export type TerminalIdParams = {
  sessionId: string;
  terminalId: string;
};

export type TerminalExitStatus = {
  exitCode: number | null;
  signal: string | null;
};

export type TerminalRecord = {
  id: string;
  sessionId: string;
  child: ChildProcessWithoutNullStreams;
  stdoutDecoder: StringDecoder;
  stderrDecoder: StringDecoder;
  output: string;
  truncated: boolean;
  outputByteLimit: number;
  exitStatus: TerminalExitStatus | undefined;
  released: boolean;
  forceKillTimer: ReturnType<typeof setTimeout> | undefined;
  waiters: Array<(status: TerminalExitStatus) => void>;
};
