import { CircleAlert, LoaderCircle, Plus } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";

import { AgentIcon } from "../AgentIcon";
import { CODEX_INTEGRATION_INSTALLING_LABEL } from "../agentActivityPresentation";
import { SettingsCatalogSearch } from "./SettingsCatalogSearch";

export function AgentSettingsList({
  agents,
  onAdd,
  onSelectAgent,
  onSetAgentEnabled,
}: {
  agents: AgentSettingsRecord[];
  onAdd: () => void;
  onSelectAgent: (agent: AgentSettingsRecord) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
}) {
  const [query, setQuery] = useState("");
  const visibleAgents = useMemo(() => filterAgents(agents, query), [agents, query]);
  const builtIn = visibleAgents.filter((agent) => agent.source_kind === "built_in");
  const custom = visibleAgents.filter((agent) => agent.source_kind === "custom");

  return (
    <section className="agent-catalog agent-library">
      <div className="agent-library-intro">
        <p>Choose the agents available when you start work. Open one to sign in, troubleshoot, or change how it runs.</p>
        <SettingsCatalogSearch
          label="Search agents"
          onChange={setQuery}
          placeholder="Search"
          value={query}
        />
      </div>
      <AgentGroup
        agents={builtIn}
        label="Built-in"
        onSelectAgent={onSelectAgent}
        onSetAgentEnabled={onSetAgentEnabled}
      />
      <AgentGroup
        action={<button className="agent-add-button" onClick={onAdd} type="button"><Plus size={14} /><span>Add agent</span></button>}
        agents={custom}
        label="Your agents"
        onSelectAgent={onSelectAgent}
        onSetAgentEnabled={onSetAgentEnabled}
      />
      {!visibleAgents.length ? (
        <div className="settings-empty">
          <strong>{query ? "No agents found" : "No agents configured"}</strong>
          <span>{query ? "Try a different search." : "Add an ACP agent to get started."}</span>
        </div>
      ) : null}
    </section>
  );
}

function AgentGroup({
  action,
  agents,
  label,
  onSelectAgent,
  onSetAgentEnabled,
}: {
  action?: ReactNode;
  agents: AgentSettingsRecord[];
  label: string;
  onSelectAgent: (agent: AgentSettingsRecord) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
}) {
  if (!agents.length && !action) return null;
  return (
    <section className="agent-catalog-group">
      <header><strong>{label}</strong>{action}</header>
      <div className="agent-catalog-list" role="list" aria-label={label}>
        {agents.map((agent) => (
          <div className="agent-library-row" key={agent.id} role="listitem">
            <button className="agent-catalog-row" onClick={() => onSelectAgent(agent)} type="button">
              <span className="agent-list-avatar" aria-hidden="true">
                <AgentIcon agentId={agent.id} agentName={agent.label} icon={agent.icon} size={17} />
              </span>
              <span className="agent-catalog-copy">
                <strong>{agent.label}</strong>
                <small>{agent.description}</small>
              </span>
              <AgentStatus agent={agent} />
            </button>
            <label className="settings-switch agent-library-toggle" aria-label={`${agent.label} available`}>
              <input
                aria-label={`${agent.label} available`}
                checked={agent.enabled}
                onChange={(event) => onSetAgentEnabled(agent.id, event.currentTarget.checked)}
                type="checkbox"
              />
              <span className="settings-switch-track" aria-hidden="true" />
            </label>
          </div>
        ))}
        {!agents.length ? <p className="agent-library-none">No custom agents yet.</p> : null}
      </div>
    </section>
  );
}

function AgentStatus({ agent }: { agent: AgentSettingsRecord }) {
  const copy = statusCopy(agent.status);
  if (!copy) return null;
  const attention = agent.status === "auth_required" || agent.status === "failed" || agent.status === "setup_required";
  const installing = agent.status === "installing";
  return (
    <span aria-busy={installing || undefined} className={`agent-library-state ${agent.status}`}>
      {installing ? <LoaderCircle aria-hidden="true" className="spin" size={13} /> : attention ? <CircleAlert size={13} /> : <i />}
      {copy}
    </span>
  );
}

function statusCopy(status: AgentSettingsRecord["status"]) {
  switch (status) {
    case "auth_required": return "Sign in required";
    case "setup_required": return "Finish setup";
    case "failed": return "Needs attention";
    case "disabled": return "Off";
    case "connected": return "Connected";
    case "ready": return "Ready";
    case "installing": return CODEX_INTEGRATION_INSTALLING_LABEL;
    case "disconnected":
    case "unprobed":
    case "launching":
      return null;
    default: return status.replaceAll("_", " ");
  }
}

function filterAgents(agents: AgentSettingsRecord[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return agents;
  return agents.filter((agent) => (
    `${agent.label} ${agent.description} ${agent.source_kind} ${agent.status}`
      .toLowerCase()
      .includes(normalized)
  ));
}
