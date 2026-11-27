export type AgentEnvironmentSecretRef = {
  kind: "agentEnvironment";
  agentId: string;
  name: string;
};

export type McpSecretRef = {
  kind: "mcp";
  serverId: string;
  field: "env" | "header";
  name: string;
};

export type SecretRef = AgentEnvironmentSecretRef | McpSecretRef;

export type SecretSyncWrite =
  | {
      target: SecretRef;
      value: string;
      copyFrom?: never;
    }
  | {
      target: SecretRef;
      copyFrom: SecretRef;
      value?: never;
    };

export type SecretSyncPayload = {
  writes: SecretSyncWrite[];
  deletes: SecretRef[];
};

export type SecretTransactionApplyMessage = {
  type: "secret.transaction.apply";
  payload: {
    requestId: string;
    transactionId: string;
    changes: SecretSyncPayload;
  };
};

export type SecretTransactionFinishMessage = {
  type: "secret.transaction.commit" | "secret.transaction.rollback";
  payload: {
    requestId: string;
    transactionId: string;
  };
};

export type SecretTransactionMessage =
  | SecretTransactionApplyMessage
  | SecretTransactionFinishMessage;

export type SecretTransactionResultMessage = {
  type: "secret.transaction.result";
  payload: {
    requestId: string;
    transactionId: string;
  } & (
    | { ok: true }
    | { ok: false; error: string }
  );
};

/** Asks the shell which secure-storage key ids it holds, so the client can declare them (ADR-0086). */
export type SecretKeysRequestMessage = {
  type: "secret.keys.request";
  payload: { requestId: string };
};

/** Key ids only, never values. `unknownHolder`: the shell may hold secrets it cannot list. */
export type SecretKeysResultMessage = {
  type: "secret.keys.result";
  payload: { requestId: string; keyIds: string[]; unknownHolder: boolean };
};
