import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentIcon } from "../AgentIcon";
import { CODEX_INTEGRATION_INSTALLING_LABEL } from "../agentActivityPresentation";
import { AgentRecoveryButtons, type AgentRecoveryActions, type AgentRecoveryKind } from "../AgentRecovery";
import { AgentEnvEditor, AgentIconPicker } from "./AgentCustomFields";
import { agentStatusCopy, type AgentDraft } from "./agentSettingsModel";
import { AgentSignIn } from "./AgentSignIn";
import { settingsInfo } from "./settingsInfo";
import {
  InfoHint,
  SettingsButton,
  SettingsCrumb,
  SettingsField,
  SettingsHealth,
  SettingsIdentity,
  SettingsInlineError,
  SettingsRowView,
  SettingsSwitch,
} from "./settingsKit";
import type { SettingsTone } from "./settingsModel";

/** What the list and the page both say about an Agent's state, in the same words. */
export function agentStatus(agent: AgentSettingsRecord): { tone: SettingsTone; text: string } | undefined {
  switch (agent.status) {
    case "auth_required": return { tone: "warn", text: "Sign in required" };
    case "setup_required": return { tone: "warn", text: "Finish setup" };
    case "failed": return { tone: "warn", text: "Needs attention" };
    case "unsupported": return { tone: "warn", text: "Unsupported" };
    case "disabled": return { tone: "off", text: "Off" };
    case "connected": return { tone: "ok", text: "Connected" };
    case "ready": return { tone: "ok", text: "Ready" };
    case "installing": return { tone: "busy", text: CODEX_INTEGRATION_INSTALLING_LABEL };
    case "launching": return { tone: "busy", text: "Starting" };
    case "authenticating": return { tone: "busy", text: "Signing in" };
    case "disconnected": return { tone: "off", text: "Not connected" };
    default: return undefined;
  }
}

/**
 * One Agent's page: its state and the next step first, then how it launches.
 * A Custom Agent is edited in place; nothing is sent until Save.
 */
export function AgentPage({
  dirty,
  draft,
  isCreating,
  onAuthenticate,
  onBack,
  onCancelAuthentication,
  onCancelDraft,
  onDelete,
  onLogout,
  onSave,
  onSetAvailable,
  onUpdateDraft,
  recoveryActions,
  saveBlockedMessage,
  saveChecksConnection,
  saveError,
  savePending,
  selected,
}: {
  dirty: boolean;
  draft: AgentDraft;
  isCreating: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
  onBack: () => void;
  onCancelAuthentication?: (agentId: string) => void | Promise<void>;
  onCancelDraft: () => void;
  onDelete: () => void;
  onLogout?: (agentId: string) => boolean | void | Promise<boolean | void>;
  onSave: () => void;
  onSetAvailable: (enabled: boolean) => void;
  onUpdateDraft: (patch: Partial<AgentDraft>) => void;
  recoveryActions?: AgentRecoveryActions;
  saveBlockedMessage?: string;
  saveChecksConnection: boolean;
  saveError?: string;
  savePending: boolean;
  selected?: AgentSettingsRecord;
}) {
  const custom = isCreating || selected?.source_kind === "custom";
  const title = isCreating ? draft.label.trim() || "New agent" : selected?.label ?? draft.label;
  const available = dirty || isCreating ? draft.enabled : selected?.enabled ?? draft.enabled;
  return (
    <section aria-label="Agent details" className="settings-detail">
      <SettingsCrumb label="Agents" onBack={onBack} />
      <SettingsIdentity
        end={<SettingsSwitch checked={available} label={`${title} available`} onChange={onSetAvailable} />}
        icon={<AgentIcon agentId={selected?.id} agentName={title} icon={draft.icon} size={26} />}
        subtitle={custom ? "Custom agent" : "Built-in agent"}
        title={title}
      />

      {selected ? <AgentHealth agent={selected} recoveryActions={recoveryActions} /> : null}
      {selected && selected.auth_methods.length > 0 ? (
        <AgentSignIn agent={selected} onAuthenticate={onAuthenticate} onCancel={onCancelAuthentication} onLogout={onLogout} />
      ) : null}

      <section className="settings-group">
        <header><h2>Launch</h2>{custom ? <InfoHint info={settingsInfo.command} /> : null}</header>
        {custom ? (
          <div className="settings-card settings-form">
            <SettingsField label="Name">
              <input aria-label="Agent name" onChange={(event) => onUpdateDraft({ label: event.currentTarget.value })} value={draft.label} />
            </SettingsField>
            <SettingsField label="Command">
              <input
                aria-label="Agent command"
                className="settings-mono"
                onChange={(event) => onUpdateDraft({ command_line: event.currentTarget.value })}
                spellCheck={false}
                value={draft.command_line}
              />
            </SettingsField>
            <div className="settings-field">
              <span>Icon</span>
              <AgentIconPicker onChange={(icon) => onUpdateDraft({ icon })} value={draft.icon} />
            </div>
          </div>
        ) : selected ? (
          <ul className="settings-card">
            <SettingsRowView row={{ id: "launch", label: "Launch", control: <span className="settings-value">{selected.launch_label}</span> }} />
            <SettingsRowView row={{ id: "transport", label: "Transport", control: <code className="settings-mono">{selected.transport}</code> }} />
          </ul>
        ) : null}
      </section>

      {custom ? <AgentEnvEditor env={draft.env} onChange={(env) => onUpdateDraft({ env })} /> : null}

      {custom ? (
        <>
          {saveError ? <SettingsInlineError>{saveError}</SettingsInlineError> : null}
          <footer className="settings-detail-footer">
            {draft.agent_id ? <SettingsButton danger onClick={onDelete} variant="quiet">Delete agent…</SettingsButton> : <span />}
            {dirty ? (
              <span className="settings-save">
                <small>{saveBlockedMessage ?? (saveChecksConnection ? "Restarts the agent" : "")}</small>
                <SettingsButton disabled={savePending} onClick={onCancelDraft} variant="quiet">Cancel</SettingsButton>
                <SettingsButton busy={savePending} disabled={Boolean(saveBlockedMessage)} onClick={onSave} variant="primary">
                  {savePending ? (saveChecksConnection ? "Saving and checking…" : "Saving…") : isCreating ? "Add agent" : "Save"}
                </SettingsButton>
              </span>
            ) : null}
          </footer>
        </>
      ) : null}
    </section>
  );
}

function AgentHealth({ agent, recoveryActions }: { agent: AgentSettingsRecord; recoveryActions?: AgentRecoveryActions }) {
  const status = agentStatus(agent) ?? { tone: "off" as const, text: "Not checked" };
  const recoveryKind = agentSettingsRecoveryKind(agent);
  return (
    <SettingsHealth
      role={agent.status === "failed" ? "alert" : "status"}
      status={status.text}
      text={(
        <>
          {agent.status === "disabled" ? "Turn it on to use it for tasks." : agent.status === "disconnected" ? null : agentStatusCopy(agent)}
          {agent.last_error_summary ? (
            <details className="settings-failure">
              <summary>Failure details</summary>
              <code>{agent.last_error_summary}</code>
            </details>
          ) : null}
        </>
      )}
      tone={status.tone}
    >
      {recoveryActions && recoveryKind
        ? <AgentRecoveryButtons actions={recoveryActions} agent={agent} kind={recoveryKind} surface="settings" />
        : null}
    </SettingsHealth>
  );
}

function agentSettingsRecoveryKind(agent: AgentSettingsRecord): AgentRecoveryKind | undefined {
  if (agent.status === "setup_required") {
    return agent.setup_reason === "nodeJsRequired" ? "nodeJsRequired" : "setupRequired";
  }
  if (agent.status === "disconnected") return "connectionCheck";
  if (agent.status === "failed") return "launchFailed";
  return undefined;
}
