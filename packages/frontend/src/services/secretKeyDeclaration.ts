import {
  CLIENT_SECRET_KEYS_CHANGED,
  type BackendConnection,
} from "@openaide/app-server-client";
import type { HostToWebviewMessage, WebviewToHostMessage } from "@openaide/app-shell-contracts";
import type { PostHostMessage } from "../state/postHostMessage";

const RESPONSE_TIMEOUT_MS = 15_000;

type DeclarationLogger = (event: string, fields: Record<string, unknown>) => void;

type SecretKeyDeclarationOptions = {
  backendConnection: Pick<BackendConnection, "notify">;
  log?: DeclarationLogger;
  postHostMessage: PostHostMessage;
  subscribeHostMessages(listener: (message: HostToWebviewMessage) => void): () => void;
};

/**
 * Declares to the App Server which secure-storage key ids this shell holds, so `secret/read`
 * is routed to a client that has the key (ADR-0086). The shell owns the keys; this asks it for
 * the key ids (never values) and sends them as a client notification. The App Server forgets a
 * declaration with its session, so `declare` runs after every (re)initialize and after a
 * secret change.
 */
export function startSecretKeyDeclaration({
  backendConnection,
  log = () => undefined,
  postHostMessage,
  subscribeHostMessages,
}: SecretKeyDeclarationOptions) {
  let disposed = false;
  let sequence = 0;

  async function declare() {
    if (disposed || !backendConnection.notify) return;
    const startedAt = Date.now();
    const requestId = `secret-keys-${crypto.randomUUID()}`;
    const operation = ++sequence;
    log("secret_keys_declare_started", { operation });
    try {
      const keyIds = await requestKeyIds(requestId);
      // A later declaration supersedes this one; sending the older set would shrink the newer.
      if (disposed || operation !== sequence) return;
      backendConnection.notify(CLIENT_SECRET_KEYS_CHANGED, { keyIds });
      log("secret_keys_declare_completed", {
        operation,
        outcome: "success",
        key_count: keyIds.length,
        duration_ms: Date.now() - startedAt,
      });
    } catch {
      log("secret_keys_declare_completed", {
        operation,
        outcome: "failure",
        error_kind: "shell_keys_unavailable",
        duration_ms: Date.now() - startedAt,
      });
    }
  }

  function requestKeyIds(requestId: string) {
    return new Promise<string[]>((resolve, reject) => {
      const timeout = setTimeout(() => {
        unsubscribe();
        reject(new Error("The shell did not list its secure-storage keys."));
      }, RESPONSE_TIMEOUT_MS);
      const unsubscribe = subscribeHostMessages((message) => {
        if (message.type !== "secret.keys.result" || message.payload.requestId !== requestId) return;
        clearTimeout(timeout);
        unsubscribe();
        resolve(message.payload.keyIds.filter((key) => typeof key === "string"));
      });
      postHostMessage({
        type: "secret.keys.request",
        payload: { requestId },
      } satisfies WebviewToHostMessage);
    });
  }

  // A saved or rolled-back secret changes the held keys; the shell's index already reflects it.
  const stopWatching = subscribeHostMessages((message) => {
    if (message.type === "secret.transaction.result" && message.payload.ok) void declare();
  });

  return {
    declare,
    dispose() {
      disposed = true;
      stopWatching();
    },
  };
}
