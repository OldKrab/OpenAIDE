import { sanitizeDiagnosticText } from "../logging/logger";
import type { RuntimeHostRequestHandler } from "./rpcClientTypes";
import type { RuntimeHostRequestMessage, RuntimeHostResponse } from "./rpcWire";

export async function runRuntimeHostRequest(
  message: RuntimeHostRequestMessage,
  handler: RuntimeHostRequestHandler | undefined,
): Promise<RuntimeHostResponse> {
  if (!handler) {
    return {
      jsonrpc: "2.0",
      id: message.id,
      error: {
        code: -32601,
        message: `Host method not found: ${message.method}`,
      },
    };
  }

  try {
    const result = await handler(message.params);
    return {
      jsonrpc: "2.0",
      id: message.id,
      result: result ?? null,
    };
  } catch (error) {
    return {
      jsonrpc: "2.0",
      id: message.id,
      error: {
        code: -32603,
        message: sanitizeDiagnosticText(error instanceof Error ? error.message : error),
      },
    };
  }
}
