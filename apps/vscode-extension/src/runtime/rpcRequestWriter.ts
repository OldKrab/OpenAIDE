import type { ChildProcessWithoutNullStreams } from "node:child_process";
import type { RequestMeta } from "@openaide/app-server-client";
import type { ExtensionLogger } from "../logging/logger";
import type { RpcId, RuntimeHostResponse } from "./rpcWire";

export type RuntimeRequestOptions = {
  timeoutMs?: number;
  meta?: RequestMeta;
};

export function createRuntimeRequestPayload(
  id: RpcId,
  method: string,
  params: unknown,
  options: RuntimeRequestOptions,
) {
  return {
    jsonrpc: "2.0",
    id,
    method,
    params,
    ...(options.meta ? { meta: options.meta } : {}),
  };
}

export function createRuntimeHostSuccessResponse(requestId: string, result: unknown): RuntimeHostResponse {
  return {
    jsonrpc: "2.0",
    id: requestId,
    result,
  };
}

export function writeRuntimeJsonLine(child: ChildProcessWithoutNullStreams, payload: unknown) {
  child.stdin.write(`${JSON.stringify(payload)}\n`);
}

export function writeRuntimeHostResponse(
  child: ChildProcessWithoutNullStreams | undefined,
  response: RuntimeHostResponse,
  logger: ExtensionLogger,
) {
  if (!child || child.killed || child.stdin.destroyed || child.stdin.writableEnded) {
    logger.warn("dropping host response because runtime stdin is closed");
    return;
  }

  try {
    writeRuntimeJsonLine(child, response);
  } catch (error) {
    logger.warn("failed to write host response", { error: String(error) });
  }
}
