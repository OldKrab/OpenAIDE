import type { ExtensionLogger } from "../logging/logger";
import type { PendingRequest, RuntimeNotification } from "./rpcClientTypes";
import {
  isRpcId,
  isRuntimeHostRequest,
  parseRuntimeRpcLine,
  runtimeRpcError,
  type RuntimeHostRequestMessage,
} from "./rpcWire";

export type RuntimeLineHandlerContext = {
  pending: Map<number | string, PendingRequest>;
  notificationListeners: Set<(notification: RuntimeNotification) => void>;
  handleHostRequest: (message: RuntimeHostRequestMessage) => void;
  logger: ExtensionLogger;
};

export function handleRuntimeLine(line: string, context: RuntimeLineHandlerContext) {
  const parsed = parseRuntimeRpcLine(line);
  if (parsed.kind === "invalid") {
    context.logger.warn("runtime emitted invalid json", { error: String(parsed.error) });
    return;
  }
  const message = parsed.message;

  if (isRuntimeHostRequest(message)) {
    context.handleHostRequest(message);
    return;
  }

  if (message.id === undefined) {
    if (typeof message.method === "string") {
      for (const listener of context.notificationListeners) {
        listener({ method: message.method, params: message.params });
      }
    } else {
      context.logger.info("runtime notification", { byteLength: line.length });
    }
    return;
  }

  if (!isRpcId(message.id)) {
    context.logger.warn("runtime emitted response with invalid id", { idType: typeof message.id });
    return;
  }

  const pending = context.pending.get(message.id);
  if (!pending) return;
  context.pending.delete(message.id);
  clearTimeout(pending.timeout);

  if (message.error) {
    pending.reject(runtimeRpcError(message.error));
  } else {
    pending.resolve(message.result);
  }
}
