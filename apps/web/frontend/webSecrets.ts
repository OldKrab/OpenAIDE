import { SECRET_READ } from "@openaide/app-server-client";
import {
  createSecretTransactionHandler,
  type AppShellSecretStore,
  type HostToWebviewMessage,
  type WebviewToHostMessage,
} from "@openaide/app-shell-contracts";

type WebSecretMessage = Extract<
  WebviewToHostMessage,
  { type: `secret.transaction.${string}` | "appServer.serverRequest" }
>;

type SafeLog = (event: string, fields: Record<string, unknown>) => void;

/** Handles the Web shell's encrypted-store writes and App Server secret reads. */
export function createWebSecretMessageHandler(
  secretStore: AppShellSecretStore,
  log: SafeLog = () => undefined,
) {
  const handleTransaction = createSecretTransactionHandler();
  return async function handleWebSecretMessage(
    message: WebSecretMessage,
  ): Promise<HostToWebviewMessage | undefined> {
    const startedAt = performance.now();
    const operation = message.type === "appServer.serverRequest"
      ? "web_secret_read"
      : message.type.replaceAll(".", "_");
    const correlation = message.type === "appServer.serverRequest"
      ? message.payload.requestId
      : message.payload.transactionId;
    log(`${operation}_started`, { correlation_id: correlation });
    try {
      const result = message.type === "appServer.serverRequest"
        ? await readSecret(message, secretStore)
        : await handleTransaction(message, secretStore);
      const failed = result?.type === "secret.transaction.result" && !result.payload.ok;
      log(`${operation}_completed`, {
        correlation_id: correlation,
        duration_ms: Math.round(performance.now() - startedAt),
        ...(failed ? { error_kind: "secure_storage" } : {}),
        outcome: failed ? "failure" : "success",
      });
      return result;
    } catch {
      log(`${operation}_completed`, {
        correlation_id: correlation,
        duration_ms: Math.round(performance.now() - startedAt),
        outcome: "failure",
        error_kind: "secure_storage",
      });
      if (message.type !== "appServer.serverRequest") throw new Error("Secure storage operation failed.");
      return {
        type: "appServer.serverRequest.result",
        payload: {
          requestId: message.payload.requestId,
          method: message.payload.method,
          result: { value: null },
        },
      };
    }
  };
}

async function readSecret(
  message: Extract<WebSecretMessage, { type: "appServer.serverRequest" }>,
  secretStore: AppShellSecretStore,
): Promise<HostToWebviewMessage | undefined> {
  if (message.payload.method !== SECRET_READ) return undefined;
  const params = message.payload.params;
  if (!params || typeof params !== "object" || Array.isArray(params)) {
    throw new Error("Secure storage request is invalid.");
  }
  const key = (params as { key?: unknown }).key;
  if (typeof key !== "string" || !key) throw new Error("Secure storage key is invalid.");
  return {
    type: "appServer.serverRequest.result",
    payload: {
      requestId: message.payload.requestId,
      method: message.payload.method,
      result: { value: await secretStore.get(key) ?? null },
    },
  };
}
