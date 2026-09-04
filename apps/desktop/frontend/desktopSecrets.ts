import { SECRET_READ } from "@openaide/app-server-client";
import {
  createSecretTransactionHandler,
  type AppShellSecretStore,
  type HostToWebviewMessage,
  type WebviewToHostMessage,
} from "@openaide/app-shell-contracts";

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type DesktopSecretMessage = Extract<
  WebviewToHostMessage,
  { type: `secret.transaction.${string}` | "appServer.serverRequest" }
>;

export function createDesktopSecretMessageHandler(invoke: Invoke) {
  const store: AppShellSecretStore = {
    delete: (key) => invoke("desktop_secret_delete", { key }).then(() => undefined),
    async get(key) {
      const value = await invoke("desktop_secret_read", { key });
      if (value === null || typeof value === "string") return value ?? undefined;
      throw new Error("Secure storage returned an invalid value.");
    },
    store: (key, value) => invoke("desktop_secret_write", { key, value }).then(() => undefined),
  };
  const handleTransaction = createSecretTransactionHandler();
  return async function handleDesktopSecretMessage(
    message: DesktopSecretMessage,
  ): Promise<HostToWebviewMessage | undefined> {
    if (message.type !== "appServer.serverRequest") {
      return handleTransaction(message, store);
    }
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
        result: { value: await readAvailableSecret(store, key) },
      },
    };
  };
}

async function readAvailableSecret(store: AppShellSecretStore, key: string) {
  try {
    return await store.get(key) ?? null;
  } catch {
    // A terminal null response lets authentication fail immediately instead of
    // leaving the App Server waiting for a native credential-provider timeout.
    return null;
  }
}
