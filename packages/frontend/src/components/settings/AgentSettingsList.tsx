import { ChevronRight, Plus } from "lucide-react";
import { useMemo, useState } from "react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";

import { AgentIcon } from "../AgentIcon";
import { SettingsCatalogSearch } from "./SettingsCatalogSearch";
import { StatusBadge } from "./settingsPresentation";

export function AgentSettingsList({
  agents,
  onAdd,
  onSelectAgent,
}: {
  agents: AgentSettingsRecord[];
  onAdd: () => void;
  onSelectAgent: (agent: AgentSettingsRecord) => void;
}) {
  const [query, setQuery] = useState("");
  const visibleAgents = useMemo(() => filterAgents(agents, query), [agents, query]);
  const attention = visibleAgents.filter(needsAttention);
  const configured = visibleAgents.filter((agent) => !needsAttention(agent));

  return (
    <section className="agent-catalog">
      <div className="settings-catalog-tools">
        <SettingsCatalogSearch
          label="Search agents"
          onChange={setQuery}
          placeholder="Search agents"
          value={query}
        />
        <button className="agent-add-button" onClick={onAdd} type="button">
          <Plus size={14} /><span>Add agent</span>
        </button>
      </div>
      <div className="agent-catalog-groups">
        <AgentGroup
          agents={attention}
          label="Needs attention"
          onSelectAgent={onSelectAgent}
          title="Agents that need action before they can be used."
        />
        <AgentGroup
          agents={configured}
          label="Agents"
          onSelectAgent={onSelectAgent}
          title="Configured agents."
        />
        {!visibleAgents.length ? (
          <div className="settings-empty">
            <strong>{query ? "No agents found" : "No agents configured"}</strong>
            <span>{query ? "Try a different search." : "Add an ACP agent to get started."}</span>
          </div>
        ) : null}
      </div>
    </section>
  );
}

function AgentGroup({
  agents,
  label,
  onSelectAgent,
  title,
}: {
  agents: AgentSettingsRecord[];
  label: string;
  onSelectAgent: (agent: AgentSettingsRecord) => void;
  title: string;
}) {
  if (!agents.length) return null;
  return (
    <section className="agent-catalog-group" title={title}>
      <header>
        <strong>{label}</strong>
        <small>{agents.length}</small>
      </header>
      <div className="agent-catalog-list" role="list" aria-label={label}>
        {agents.map((agent) => (
          <button
            className="agent-catalog-row"
            key={agent.id}
            onClick={() => onSelectAgent(agent)}
            role="listitem"
            type="button"
          >
            <span className="agent-list-avatar" aria-hidden="true">
              <AgentIcon icon={agent.icon} size={15} />
            </span>
            <span className="agent-catalog-copy">
              <strong>{agent.label}</strong>
              <small>{agent.description}</small>
            </span>
            <span className="agent-catalog-trailing">
              <span className="agent-catalog-source">
                {agent.source_kind === "built_in" ? "Built-in" : "Custom"}
              </span>
              {agent.status === "ready" || agent.status === "connected"
                ? null
                : <StatusBadge status={agent.status} />}
              <ChevronRight aria-hidden="true" size={14} />
            </span>
          </button>
        ))}
      </div>
    </section>
  );
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

function needsAttention(agent: AgentSettingsRecord) {
  return agent.status === "auth_required"
    || agent.status === "failed"
    || agent.status === "setup_required";
}
