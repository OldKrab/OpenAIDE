import type { CustomAgentEnvRecord } from "@openaide/app-shell-contracts";

export type AgentLaunchIdentity = {
  command_line?: string;
  env?: CustomAgentEnvRecord[];
};

export function sameAgentLaunchIdentity(left: AgentLaunchIdentity, right: AgentLaunchIdentity) {
  return (left.command_line ?? "") === (right.command_line ?? "")
    && envFingerprint(left.env ?? []) === envFingerprint(right.env ?? []);
}

function envFingerprint(rows: CustomAgentEnvRecord[]) {
  return rows
    .map((row) => `${row.name}\u0000${row.secret ? "1" : "0"}\u0000${row.value ?? ""}`)
    .sort()
    .join("\u0001");
}
