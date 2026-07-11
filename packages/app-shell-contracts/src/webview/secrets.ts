export type AgentEnvironmentSecretRef = {
  kind: "agentEnvironment";
  agentId: string;
  name: string;
};

export type SecretSyncWrite =
  | {
      target: AgentEnvironmentSecretRef;
      value: string;
      copyFrom?: never;
    }
  | {
      target: AgentEnvironmentSecretRef;
      copyFrom: AgentEnvironmentSecretRef;
      value?: never;
    };

export type SecretSyncPayload = {
  writes: SecretSyncWrite[];
  deletes: AgentEnvironmentSecretRef[];
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
