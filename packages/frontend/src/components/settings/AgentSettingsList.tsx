import { Plus } from "lucide-react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentIcon } from "../AgentIcon";
import { StatusBadge } from "./settingsPresentation";

export function AgentSettingsList({
  agents,
  draftActive,
  onAdd,
  onSelectAgent,
  selectedId,
}: {
  agents: AgentSettingsRecord[];
  draftActive: boolean;
  onAdd: () => void;
  onSelectAgent: (agent: AgentSettingsRecord) => void;
  selectedId?: string;
}) {
  return (
    <div className="agent-master-pane">
      <div className="settings-panel-title agent-list-title">
        <span>
          <strong>Agents</strong>
          <small>
            {draftActive
              ? "Save or cancel changes before selecting another agent."
              : "Select an agent to inspect setup and launch policy."}
          </small>
        </span>
        <button
          disabled={draftActive}
          onClick={onAdd}
          title={draftActive ? "Save or cancel current changes first" : undefined}
          type="button"
        >
          <Plus size={13} />
          Add agent
        </button>
      </div>
      <div className="agent-settings-list" role="list" aria-label="Agents">
        {agents.map((agent) => (
          <button
            className={agent.id === selectedId ? "selected" : ""}
            disabled={draftActive}
            key={agent.id}
            onClick={() => onSelectAgent(agent)}
            title={draftActive ? "Save or cancel current changes first" : undefined}
            type="button"
          >
            <span className="agent-list-avatar" aria-hidden="true">
              <AgentIcon icon={agent.icon} size={15} />
            </span>
            <span>
              <strong>{agent.label}</strong>
              <small>{agent.description}</small>
            </span>
            <StatusBadge status={agent.status} />
          </button>
        ))}
      </div>
    </div>
  );
}
