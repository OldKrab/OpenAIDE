import type {
  HostToWebviewMessage,
  SecretSyncPayload,
  WebviewToHostMessage,
} from "@openaide/app-shell-contracts";
import { postHostMessage, subscribeHostMessages } from "./hostBridge";

const RESPONSE_TIMEOUT_MS = 15_000;

export type AgentSecretTransaction = {
  commit(): Promise<void>;
  rollback(): Promise<void>;
};

export async function beginAgentSecretTransaction(
  changes: SecretSyncPayload,
): Promise<AgentSecretTransaction> {
  const transactionId = `secret-transaction-${crypto.randomUUID()}`;
  await requestSecretTransaction({
    type: "secret.transaction.apply",
    payload: {
      requestId: requestId(),
      transactionId,
      changes,
    },
  });
  let finished = false;
  return {
    async commit() {
      if (finished) return;
      await finish("secret.transaction.commit", transactionId);
      finished = true;
    },
    async rollback() {
      if (finished) return;
      await finish("secret.transaction.rollback", transactionId);
      finished = true;
    },
  };
}

async function finish(
  type: "secret.transaction.commit" | "secret.transaction.rollback",
  transactionId: string,
) {
  await requestSecretTransaction({
    type,
    payload: { requestId: requestId(), transactionId },
  });
}

async function requestSecretTransaction(
  message: Extract<WebviewToHostMessage, { type: `secret.transaction.${string}` }>,
) {
  const result = await new Promise<Extract<HostToWebviewMessage, { type: "secret.transaction.result" }>>(
    (resolve, reject) => {
      const timeout = setTimeout(() => {
        unsubscribe();
        reject(new Error("Secure storage did not respond."));
      }, RESPONSE_TIMEOUT_MS);
      const unsubscribe = subscribeHostMessages((response) => {
        if (
          response.type !== "secret.transaction.result" ||
          response.payload.requestId !== message.payload.requestId
        ) return;
        clearTimeout(timeout);
        unsubscribe();
        resolve(response);
      });
      postHostMessage(message);
    },
  );
  if (!result.payload.ok) throw new Error(result.payload.error);
}

function requestId() {
  return `secret-request-${crypto.randomUUID()}`;
}
