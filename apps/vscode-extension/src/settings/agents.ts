export function customAgentSecretKey(agentId: string, name: string) {
  return `openaide.agent.${agentId}.env.${name}`;
}
