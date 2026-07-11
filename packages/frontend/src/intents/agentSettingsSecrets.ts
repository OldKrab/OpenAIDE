import type {
  AgentEnvironmentSecretRef,
  CustomAgentCreateParams,
  SecretSyncPayload,
  SecretSyncWrite,
} from "@openaide/app-shell-contracts";

export function secretsForCreatedAgent(
  agentId: string,
  payload: CustomAgentCreateParams,
): SecretSyncPayload | undefined {
  return syncPayload(secretWrites(agentId, payload), []);
}

export function secretsForReplacedAgent(
  sourceAgentId: string,
  targetAgentId: string,
  sourceNames: string[],
  payload: CustomAgentCreateParams,
): SecretSyncPayload | undefined {
  const uniqueSourceNames = uniqueNames(sourceNames);
  const writes = secretWrites(targetAgentId, payload, {
    agentId: sourceAgentId,
    names: new Set(uniqueSourceNames),
  });
  return syncPayload(writes, uniqueSourceNames.map((name) => secretRef(sourceAgentId, name)));
}

export function secretsForDeletedAgent(
  agentId: string,
  names: string[],
): SecretSyncPayload | undefined {
  return syncPayload([], uniqueNames(names).map((name) => secretRef(agentId, name)));
}

function secretWrites(
  targetAgentId: string,
  payload: CustomAgentCreateParams,
  source?: { agentId: string; names: Set<string> },
): SecretSyncWrite[] {
  const writes: SecretSyncWrite[] = [];
  for (const row of payload.env) {
    if (!row.secret) continue;
    const target = secretRef(targetAgentId, row.name);
    if (row.value) {
      writes.push({ target, value: row.value });
    } else if (source?.names.has(row.name)) {
      writes.push({ target, copyFrom: secretRef(source.agentId, row.name) });
    }
  }
  return writes;
}

function secretRef(agentId: string, name: string): AgentEnvironmentSecretRef {
  return { kind: "agentEnvironment", agentId, name };
}

function uniqueNames(names: string[]) {
  return [...new Set(names)];
}

function syncPayload(
  writes: SecretSyncWrite[],
  deletes: AgentEnvironmentSecretRef[],
): SecretSyncPayload | undefined {
  return writes.length || deletes.length ? { writes, deletes } : undefined;
}
