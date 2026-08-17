import { useState, type ReactNode } from "react";
import {
  Bot,
  CircleAlert,
  FileText,
  KeyRound,
  Palette,
  Save,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentIcon } from "../AgentIcon";
import { AgentRecoveryButtons, type AgentRecoveryActions, type AgentRecoveryKind } from "../AgentRecovery";
import { AgentEnvEditor, AgentIconPicker } from "./AgentCustomFields";
import type { AgentDraft } from "./agentSettingsModel";
import { agentStatusCopy, type AgentAuthMethod } from "./agentSettingsModel";
import { InlineFailure, InlineNotice } from "./settingsPresentation";

export function AgentSettingsDetail({
  activeDraft,
  authPending,
  confirmDeleteAgentId,
  confirmReplaceAgentId,
  isCreating,
  isCustom,
  isEditing = false,
  onAuthenticate,
  onCancelDraft,
  onDeleteClick,
  onSaveDraft,
  saveChecksConnection,
  saveBlockedMessage,
  savePending,
  onSetAgentEnabled,
  onUpdateDraft,
  recoveryActions,
  selected,
}: {
  activeDraft: AgentDraft;
  authPending: boolean;
  confirmDeleteAgentId?: string;
  confirmReplaceAgentId?: string;
  isCreating: boolean;
  isCustom: boolean;
  isEditing?: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
  onCancelDraft?: () => void;
  onDeleteClick: () => void;
  onSaveDraft: () => void;
  saveChecksConnection: boolean;
  saveBlockedMessage?: string;
  savePending: boolean;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onUpdateDraft: (patch: Partial<AgentDraft>) => void;
  recoveryActions?: AgentRecoveryActions;
  selected?: AgentSettingsRecord;
}) {
  const showReplaceConfirmation = Boolean(activeDraft.agent_id && confirmReplaceAgentId === activeDraft.agent_id);
  const title = isCreating ? "Add Custom Agent" : selected?.label ?? activeDraft.label;
  const description = isCreating ? "Connect an ACP agent to OpenAIDE." : selected?.description ?? "Custom ACP agent";
  const setAvailable = (enabled: boolean) => {
    if (isCreating || (isCustom && isEditing)) {
      onUpdateDraft({ enabled });
      return;
    }
    if (selected) onSetAgentEnabled(selected.id, enabled);
  };

  return (
    <section className="agent-detail-pane agent-page" aria-label="Agent details">
      <header className="agent-page-header">
        <span className="agent-page-avatar" aria-hidden="true"><AgentIcon agentId={selected?.id} agentName={title} icon={activeDraft.icon} size={25} /></span>
        <span className="agent-page-title">
          <strong>{title}</strong>
          <small>{description}</small>
          {selected ? <AgentStatusText agent={selected} /> : null}
        </span>
        {!isCreating ? (
          <span className="agent-page-header-toggle">
            <SettingsSwitch checked={selected?.enabled ?? activeDraft.enabled} label={`${title} available`} onChange={setAvailable} />
            <small>{(selected?.enabled ?? activeDraft.enabled) ? "Available" : "Unavailable"}</small>
          </span>
        ) : null}
      </header>

      {selected && (selected.auth_methods.length > 0 || needsStatusDetail(selected)) ? (
        <AgentAttentionSection agent={selected} authPending={authPending} onAuthenticate={onAuthenticate} recoveryActions={recoveryActions} />
      ) : null}

      {isCustom ? (
        <>
          <AgentPageSection label="Identity">
            <div className="agent-page-surface">
              <AgentPageRow
                action={<input className="agent-page-inline-input" aria-label="Agent name" value={activeDraft.label} onChange={(event) => onUpdateDraft({ label: event.currentTarget.value })} />}
                icon={<Bot size={16} />}
                label="Name"
              />
              {isCreating ? (
                <AgentPageRow action={<SettingsSwitch checked={activeDraft.enabled} label="Agent available" onChange={setAvailable} />} icon={<Bot size={16} />} label="Available" />
              ) : null}
            </div>
          </AgentPageSection>
          <AgentPageSection description="Command used to start the ACP connection." label="Launch">
            <div className="agent-page-surface">
              <AgentPageRow
                action={<input className="agent-page-inline-input mono" aria-label="Agent command" value={activeDraft.command_line} onChange={(event) => onUpdateDraft({ command_line: event.currentTarget.value })} />}
                icon={<Terminal size={16} />}
                label="Command"
              />
            </div>
          </AgentPageSection>
          <AgentPageSection label="Appearance">
            <div className="agent-page-surface">
              <AgentPageRow action={<AgentIconPicker value={activeDraft.icon} onChange={(icon) => onUpdateDraft({ icon })} />} icon={<Palette size={16} />} label="Icon" />
            </div>
          </AgentPageSection>
          <AgentEnvEditor env={activeDraft.env} onChange={(env) => onUpdateDraft({ env })} />
          <footer className="agent-page-footer">
            {activeDraft.agent_id ? (
              <button className="danger" disabled={authPending} type="button" onClick={onDeleteClick}>
                <Trash2 size={13} />
                {confirmDeleteAgentId === activeDraft.agent_id ? "Confirm delete" : "Delete"}
              </button>
            ) : null}
            {onCancelDraft ? <button disabled={authPending} type="button" onClick={onCancelDraft}><X size={13} /><span>Cancel</span></button> : null}
            <button
              aria-busy={savePending}
              className="primary"
              disabled={authPending || savePending || Boolean(saveBlockedMessage)}
              type="button"
              onClick={onSaveDraft}
            >
              <Save size={13} />
              {savePending
                ? (saveChecksConnection ? "Saving and checking…" : "Saving…")
                : (showReplaceConfirmation ? "Confirm replace" : "Save")}
            </button>
          </footer>
        </>
      ) : selected ? (
        <AgentPageSection description="Managed by OpenAIDE." label="Connection">
          <div className="agent-page-surface">
            <AgentPageRow action={<span className="agent-page-value">{selected.launch_label}</span>} icon={<FileText size={16} />} label="Launch policy" />
            <AgentPageRow action={<span className="agent-page-value mono">{selected.transport}</span>} icon={<Terminal size={16} />} label="Transport" />
          </div>
        </AgentPageSection>
      ) : null}

      {saveBlockedMessage ? <InlineNotice message={saveBlockedMessage} /> : null}
      {showReplaceConfirmation ? <InlineNotice message="Launch changes create a new Agent identity and remove this custom Agent." /> : null}
    </section>
  );
}

function agentSettingsRecoveryKind(agent: AgentSettingsRecord): AgentRecoveryKind | undefined {
  if (agent.status === "setup_required") {
    return agent.setup_reason === "nodeJsRequired" ? "nodeJsRequired" : "setupRequired";
  }
  if (agent.status === "disconnected") return "launchFailed";
  if (agent.status === "failed") return "launchFailed";
  return undefined;
}

function AgentPageSection({ children, description, label }: { children: ReactNode; description?: string; label: string }) {
  return (
    <section className="agent-page-section">
      <header><strong>{label}</strong>{description ? <small>{description}</small> : null}</header>
      {children}
    </section>
  );
}

export function AgentPageRow({ action, detail, icon, label }: { action: ReactNode; detail?: string; icon: ReactNode; label: string }) {
  return (
    <div className="agent-page-row">
      <span className="agent-page-row-icon">{icon}</span>
      <span className="agent-page-row-copy"><strong>{label}</strong>{detail ? <small>{detail}</small> : null}</span>
      {action}
    </div>
  );
}

function AgentStatusText({ agent }: { agent: AgentSettingsRecord }) {
  const attention = needsAttention(agent);
  return <span className={`agent-page-status ${agent.status}`}>{attention ? <CircleAlert size={12} /> : <i />}{shortStatus(agent.status)}</span>;
}

function AgentAttentionSection({
  agent,
  authPending,
  onAuthenticate,
  recoveryActions,
}: {
  agent: AgentSettingsRecord;
  authPending: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
  recoveryActions?: AgentRecoveryActions;
}) {
  const recoveryKind = agentSettingsRecoveryKind(agent);
  if (agent.auth_methods.length) {
    return (
      <AgentPageSection label="Authentication">
        <div className="agent-page-surface attention">
          {agent.auth_methods.map((method) => (
            <AgentAuthenticationMethod
              key={method.id}
              agentId={agent.id}
              agentStatus={agent.status}
              authenticatingMethodId={agent.authenticating_method_id}
              authPending={authPending}
              method={method}
              onAuthenticate={onAuthenticate}
            />
          ))}
        </div>
      </AgentPageSection>
    );
  }
  return (
    <AgentPageSection label="Setup">
      <div className="agent-page-surface attention">
        <AgentPageRow
          action={recoveryActions && recoveryKind ? <AgentRecoveryButtons actions={recoveryActions} agent={agent} kind={recoveryKind} surface="settings" /> : <span />}
          detail={authPending && agent.status === "authenticating" ? "Authentication is running." : agentStatusCopy(agent)}
          icon={<CircleAlert size={16} />}
          label="Action required"
        />
        {agent.last_error_summary ? <InlineFailure message={agent.last_error_summary} /> : null}
      </div>
    </AgentPageSection>
  );
}

function AgentAuthenticationMethod({
  agentId,
  agentStatus,
  authenticatingMethodId,
  authPending,
  method,
  onAuthenticate,
}: {
  agentId: string;
  agentStatus: AgentSettingsRecord["status"];
  authenticatingMethodId?: string;
  authPending: boolean;
  method: AgentAuthMethod;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const variables = method.variables ?? [];
  const missingRequired = variables.some((variable) => !variable.optional && !(values[variable.name] ?? "").trim());
  const awaitingThisTerminal = method.kind === "terminal" && agentStatus === "authenticating" && authenticatingMethodId === method.id;
  const anotherMethodIsAuthenticating = agentStatus === "authenticating" && !awaitingThisTerminal;
  return (
    <div className="agent-page-auth-method">
      <AgentPageRow
        action={(
          <button
            className="agent-page-row-button"
            disabled={authPending || anotherMethodIsAuthenticating || (method.kind === "env_var" && missingRequired)}
            type="button"
            onClick={() => onAuthenticate(agentId, method.id, method.kind === "env_var" ? values : undefined)}
          >
            <KeyRound size={13} /><span>{awaitingThisTerminal ? "I've finished signing in" : method.label}</span>
          </button>
        )}
        detail={method.description}
        icon={<KeyRound size={16} />}
        label={method.label}
      />
      {variables.map((variable) => (
        <label className="agent-page-auth-field" key={variable.name}>
          <span>{variable.label ?? variable.name}{variable.optional ? " (optional)" : ""}</span>
          <input
            aria-label={variable.label ?? variable.name}
            autoComplete="off"
            type={variable.secret ? "password" : "text"}
            value={values[variable.name] ?? ""}
            onChange={(event) => setValues((current) => ({ ...current, [variable.name]: event.currentTarget.value }))}
          />
        </label>
      ))}
    </div>
  );
}

function SettingsSwitch({ checked, label, onChange }: { checked: boolean; label: string; onChange(value: boolean): void }) {
  return (
    <label className="settings-switch agent-page-toggle" aria-label={label}>
      <input aria-label={label} checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} type="checkbox" />
      <span className="settings-switch-track" aria-hidden="true" />
    </label>
  );
}

function needsAttention(agent: AgentSettingsRecord) {
  return agent.status === "auth_required" || agent.status === "failed" || agent.status === "setup_required";
}

function needsStatusDetail(agent: AgentSettingsRecord) {
  return needsAttention(agent) || agent.status === "unprobed" || agent.status === "disconnected" || agent.status === "authenticating";
}

function shortStatus(status: AgentSettingsRecord["status"]) {
  switch (status) {
    case "auth_required": return "Sign in required";
    case "setup_required": return "Finish setup";
    case "failed": return "Needs attention";
    case "disabled": return "Disabled";
    case "connected": return "Connected";
    case "ready": return "Ready";
    default: return status.replaceAll("_", " ");
  }
}
