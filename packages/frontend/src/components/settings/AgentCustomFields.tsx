import { Plus, Trash2 } from "lucide-react";
import { agentIconIds } from "@openaide/app-shell-contracts";
import type {
  AgentIconId,
  CustomAgentEnvRecord,
} from "@openaide/app-shell-contracts";
import { AgentIcon, agentIconLabels } from "../AgentIcon";
import { InlineNotice } from "./settingsPresentation";

const customAgentIconIds = agentIconIds.filter((icon) => icon !== "openai" && icon !== "opencode");

export function AgentIconPicker({ value, onChange }: { value: AgentIconId; onChange: (icon: AgentIconId) => void }) {
  const selectedIcon: AgentIconId = customAgentIconIds.some((icon) => icon === value) ? value : "bot";
  return (
    <div className="agent-icon-picker" role="radiogroup" aria-label="Agent icon">
      {customAgentIconIds.map((icon) => (
        <button
          aria-checked={icon === selectedIcon}
          aria-label={agentIconLabels[icon]}
          className={icon === selectedIcon ? "selected" : ""}
          key={icon}
          onClick={() => onChange(icon)}
          role="radio"
          title={agentIconLabels[icon]}
          type="button"
        >
          <AgentIcon icon={icon} size={15} />
        </button>
      ))}
    </div>
  );
}

export function AgentEnvEditor({ env, onChange }: { env: CustomAgentEnvRecord[]; onChange: (env: CustomAgentEnvRecord[]) => void }) {
  const update = (index: number, patch: Partial<CustomAgentEnvRecord>) => {
    onChange(env.map((row, rowIndex) => (rowIndex === index ? { ...row, ...patch } : row)));
  };
  return (
    <section className="agent-detail-section">
      <div className="settings-section-title">
        <strong>Environment</strong>
        <button type="button" onClick={() => onChange([...env, { name: "", value: "", secret: false }])}>
          <Plus size={12} />
          Add variable
        </button>
      </div>
      {env.length ? (
        <div className="agent-env-list">
          {env.map((row, index) => (
            <div className="agent-env-row" key={index}>
              <input aria-label="Name" value={row.name} onChange={(event) => update(index, { name: event.currentTarget.value })} placeholder="NAME" />
              <input
                aria-label="Value"
                value={row.value ?? ""}
                onChange={(event) => update(index, { value: event.currentTarget.value })}
                placeholder={row.secret ? "Stored secret" : "Value"}
                type={row.secret ? "password" : "text"}
              />
              <label className="settings-switch">
                <input checked={row.secret} onChange={(event) => update(index, { secret: event.currentTarget.checked, value: "" })} type="checkbox" />
                <span className="settings-switch-track" aria-hidden="true" />
                <span>Secret</span>
              </label>
              <button type="button" aria-label="Remove environment variable" onClick={() => onChange(env.filter((_, rowIndex) => rowIndex !== index))}>
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <InlineNotice message="No environment variables." />
      )}
    </section>
  );
}
