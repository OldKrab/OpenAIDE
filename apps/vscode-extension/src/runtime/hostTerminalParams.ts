import { firstWorkspaceRoot } from "../workspace/roots";
import { validatedWorkspacePath } from "./workspaceBoundary";
import type { CreateTerminalParams, EnvVariable, TerminalIdParams } from "./hostTerminalTypes";

const DEFAULT_OUTPUT_BYTE_LIMIT = 1_048_576;
const MAX_OUTPUT_BYTE_LIMIT = 10 * 1_048_576;

export async function parseCreateTerminalParams(params: unknown): Promise<CreateTerminalParams> {
  const object = objectParams(params);
  const command = requiredString(object, "command");
  if (!command.trim()) {
    throw new Error("command must not be empty");
  }
  const cwd = await validatedWorkspacePath(optionalString(object, "cwd") ?? firstWorkspaceRoot(), "existing");
  return {
    sessionId: requiredString(object, "sessionId"),
    command,
    args: optionalStringArray(object, "args"),
    env: optionalEnv(object),
    cwd,
    outputByteLimit: outputByteLimit(object),
  };
}

export function parseTerminalIdParams(params: unknown): TerminalIdParams {
  const object = objectParams(params);
  return {
    sessionId: requiredString(object, "sessionId"),
    terminalId: requiredString(object, "terminalId"),
  };
}

function objectParams(params: unknown): Record<string, unknown> {
  if (!params || typeof params !== "object" || Array.isArray(params)) {
    throw new Error("params must be an object");
  }
  return params as Record<string, unknown>;
}

function requiredString(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (typeof value !== "string") {
    throw new Error(`${key} must be a string`);
  }
  return value;
}

function optionalString(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (value === undefined) return undefined;
  if (typeof value !== "string") {
    throw new Error(`${key} must be a string`);
  }
  return value;
}

function optionalStringArray(object: Record<string, unknown>, key: string) {
  const value = object[key];
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    throw new Error(`${key} must be an array of strings`);
  }
  return value;
}

function optionalEnv(object: Record<string, unknown>) {
  const value = object.env;
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    throw new Error("env must be an array");
  }
  return value.map((item, index): EnvVariable => {
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      throw new Error(`env.${index} must be an object`);
    }
    const entry = item as Record<string, unknown>;
    return {
      name: requiredString(entry, "name"),
      value: requiredString(entry, "value"),
    };
  });
}

function outputByteLimit(object: Record<string, unknown>) {
  const value = object.outputByteLimit;
  if (value === undefined) return DEFAULT_OUTPUT_BYTE_LIMIT;
  if (!Number.isInteger(value) || (value as number) < 0) {
    throw new Error("outputByteLimit must be a non-negative integer");
  }
  return Math.min(value as number, MAX_OUTPUT_BYTE_LIMIT);
}
