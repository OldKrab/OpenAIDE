import { protocolErrorFromUnknown } from "@openaide/app-server-client";

export type RpcId = number | string;

export type RuntimeRpcMessage = {
  id?: unknown;
  method?: unknown;
  params?: unknown;
  result?: unknown;
  error?: unknown;
};

export type RuntimeHostRequestMessage = {
  id: RpcId;
  method: string;
  params?: unknown;
};

export type RuntimeHostResponse = {
  jsonrpc: "2.0";
  id: RpcId;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
};

export type RuntimeRpcParseResult =
  | { kind: "parsed"; message: RuntimeRpcMessage }
  | { kind: "invalid"; error: Error };

export function parseRuntimeRpcLine(line: string): RuntimeRpcParseResult {
  try {
    return { kind: "parsed", message: JSON.parse(line) as RuntimeRpcMessage };
  } catch (error) {
    return { kind: "invalid", error: error instanceof Error ? error : new Error(String(error)) };
  }
}

export function isRpcId(value: unknown): value is RpcId {
  return typeof value === "number" || typeof value === "string";
}

export function isRuntimeHostRequest(message: RuntimeRpcMessage): message is RuntimeHostRequestMessage {
  return isRpcId(message.id) && typeof message.method === "string";
}

export function runtimeRpcError(error: unknown): Error {
  return protocolErrorFromUnknown(error);
}
