import { KeyRound, Plus } from "lucide-react";
import type {
  AgentIconId,
  CustomAgentEnvRecord,
} from "@openaide/app-shell-contracts";
import { agentIconIds } from "@openaide/app-shell-contracts";
import { AgentIcon, agentIconLabels } from "../AgentIcon";
import { SettingsButton, SettingsSwitch } from "./settingsKit";

/** Built-in brand marks stay reserved for their Agents; custom Agents pick from the rest. */
const reservedCustomAgentIconIds: AgentIconId[] = ["openai", "opencode"];
const preferredCustomAgentIconIds: AgentIconId[] = ["bot", "code", "terminal", "sparkles", "wrench", "brain"];
const pickableCustomAgentIconIds: AgentIconId[] = [
  ...preferredCustomAgentIconIds,
  ...agentIconIds.filter((icon) => (
    !preferredCustomAgentIconIds.includes(icon) && !reservedCustomAgentIconIds.includes(icon)
  )),
];

export function AgentIconPicker({ value, onChange }: { value: AgentIconId; onChange: (icon: AgentIconId) => void }) {
  // A record saved before this list grew may hold an icon outside it. Keep the
  // current choice visible so the picker never silently reports another icon.
  const iconIds = pickableCustomAgentIconIds.includes(value)
    ? pickableCustomAgentIconIds
    : [value, ...pickableCustomAgentIconIds];
  return (
    <div className="settings-icon-picker" role="radiogroup" aria-label="Agent icon">
      {iconIds.map((icon) => (
        <button
          aria-checked={icon === value}
          aria-label={agentIconLabels[icon]}
          key={icon}
          onClick={() => onChange(icon)}
          role="radio"
          title={agentIconLabels[icon]}
          type="button"
        >
          <AgentIcon icon={icon} size={16} />
        </button>
      ))}
    </div>
  );
}

/** Variables passed to a Custom Agent's process. A secret is write-only: its stored value never returns. */
export function AgentEnvEditor({ env, onChange }: { env: CustomAgentEnvRecord[]; onChange: (env: CustomAgentEnvRecord[]) => void }) {
  const update = (index: number, patch: Partial<CustomAgentEnvRecord>) => {
    onChange(env.map((row, rowIndex) => (rowIndex === index ? { ...row, ...patch } : row)));
  };
  return (
    <section className="settings-group">
      <header>
        <h2>Environment</h2>
        <span className="settings-group-actions">
          <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={() => onChange([...env, { name: "", value: "", secret: false }])}>
            Add variable
          </SettingsButton>
        </span>
      </header>
      {env.length ? (
        <ul className="settings-card settings-env">
          {env.map((row, index) => (
            <li key={index}>
              <input aria-label="Name" className="settings-mono" value={row.name} onChange={(event) => update(index, { name: event.currentTarget.value })} placeholder="NAME" />
              <input
                aria-label="Value"
                className="settings-mono"
                value={row.value ?? ""}
                onChange={(event) => update(index, { value: event.currentTarget.value })}
                placeholder={row.secret ? "Stored secret" : "Value"}
                type={row.secret ? "password" : "text"}
              />
              <span className="settings-env-secret">
                <KeyRound aria-hidden="true" size={13} />Secret
                <SettingsSwitch checked={row.secret} label={`${row.name || "Variable"} is secret`} onChange={(secret) => update(index, { secret, value: "" })} />
              </span>
              <SettingsButton ariaLabel="Remove environment variable" onClick={() => onChange(env.filter((_, rowIndex) => rowIndex !== index))} variant="quiet">Remove</SettingsButton>
            </li>
          ))}
        </ul>
      ) : <p className="settings-empty">None.</p>}
    </section>
  );
}
