import { useState } from "react";
import { FileText, LockKeyhole, Repeat2, Save, Trash2, X } from "lucide-react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentIcon } from "../AgentIcon";
import { AgentEnvEditor, AgentIconPicker } from "./AgentCustomFields";
import type { AgentDraft } from "./agentSettingsModel";
import { agentStatusCopy, type AgentAuthMethod } from "./agentSettingsModel";
import { InlineFailure, InlineNotice, StatusBadge } from "./settingsPresentation";

export function AgentSettingsDetail({
  activeDraft,
  authPending,
  confirmDeleteAgentId,
  confirmReplaceAgentId,
  isCreating,
  isCustom,
  onAuthenticate,
  onCancelDraft,
  onDeleteClick,
  onSaveDraft,
  saveBlockedMessage,
  onSetAgentEnabled,
  onUpdateDraft,
  selected,
}: {
  activeDraft: AgentDraft;
  authPending: boolean;
  confirmDeleteAgentId?: string;
  confirmReplaceAgentId?: string;
  isCreating: boolean;
  isCustom: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
  onCancelDraft?: () => void;
  onDeleteClick: () => void;
  onSaveDraft: () => void;
  saveBlockedMessage?: string;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onUpdateDraft: (patch: Partial<AgentDraft>) => void;
  selected?: AgentSettingsRecord;
}) {
  const showReplaceConfirmation = Boolean(activeDraft.agent_id && confirmReplaceAgentId === activeDraft.agent_id);
  return (
    <section className="agent-detail-pane" aria-label="Agent details">
      <header className="agent-detail-header">
        <div className="agent-detail-identity">
          <span className="agent-detail-avatar" aria-hidden="true">
            <AgentIcon icon={activeDraft.icon} size={24} />
          </span>
          <span>
            <span className="agent-title-line">
              <strong>{isCustom ? (isCreating ? "Add Custom Agent" : "Edit Custom Agent") : selected?.label}</strong>
              <span className="agent-source-badge">{isCustom ? "Custom" : "Built-in"}</span>
            </span>
            <small>{isCustom ? "Custom ACP stdio Agent" : selected?.description}</small>
          </span>
        </div>
      </header>
      {selected ? (
        <AgentStatusPanel agent={selected} authPending={authPending} />
      ) : null}
      {selected?.auth_methods.length ? (
        <AgentAuthenticationSection
          agent={selected}
          authPending={authPending}
          onAuthenticate={onAuthenticate}
        />
      ) : null}
      <section className="agent-detail-section">
        <div className="settings-section-title">
          <strong>Launch</strong>
        </div>
        {isCustom ? (
          <>
            <label className="agent-field">
              <span>Name</span>
              <input aria-label="Agent name" value={activeDraft.label} onChange={(event) => onUpdateDraft({ label: event.currentTarget.value })} />
            </label>
            <label className="agent-field">
              <span>Command</span>
              <input
                aria-label="Agent command"
                value={activeDraft.command_line}
                onChange={(event) => onUpdateDraft({ command_line: event.currentTarget.value })}
              />
            </label>
            <label className="settings-switch agent-enabled-toggle">
              <input checked={activeDraft.enabled} onChange={(event) => onUpdateDraft({ enabled: event.currentTarget.checked })} type="checkbox" />
              <span className="settings-switch-track" aria-hidden="true" />
              <span>Enabled</span>
            </label>
            <label className="agent-field">
              <span>Icon</span>
              <AgentIconPicker value={activeDraft.icon} onChange={(icon) => onUpdateDraft({ icon })} />
            </label>
          </>
        ) : (
          <AgentReadonlyRows selected={selected} />
        )}
      </section>
      {isCustom ? <AgentEnvEditor env={activeDraft.env} onChange={(env) => onUpdateDraft({ env })} /> : null}
      {!isCustom && selected ? (
        <AgentAvailabilitySection agent={selected} onSetAgentEnabled={onSetAgentEnabled} />
      ) : null}
      {isCustom ? (
        <div className="agent-detail-actions">
          <button disabled={authPending || Boolean(saveBlockedMessage)} type="button" onClick={onSaveDraft}>
            <Save size={13} />
            {showReplaceConfirmation ? "Confirm replace" : "Save"}
          </button>
          {activeDraft.agent_id ? (
            <button className="danger" disabled={authPending} type="button" onClick={onDeleteClick}>
              <Trash2 size={13} />
              {confirmDeleteAgentId === activeDraft.agent_id ? "Confirm delete" : "Delete"}
            </button>
          ) : null}
          {onCancelDraft ? (
            <button disabled={authPending} type="button" onClick={onCancelDraft}>
              <X size={13} />
              Cancel
            </button>
          ) : null}
        </div>
      ) : null}
      {saveBlockedMessage ? <InlineNotice message={saveBlockedMessage} /> : null}
      {showReplaceConfirmation ? (
        <InlineNotice message="Launch changes create a new Agent identity and remove this custom Agent." />
      ) : null}
    </section>
  );
}

function AgentReadonlyRows({ selected }: { selected?: AgentSettingsRecord }) {
  return (
    <div className="agent-readonly-rows">
      <div className="agent-readonly-row">
        <FileText size={16} />
        <span>Policy</span>
        <strong>{selected?.launch_label}</strong>
      </div>
      <div className="agent-readonly-row">
        <Repeat2 size={16} />
        <span>Transport</span>
        <strong>{selected?.transport}</strong>
      </div>
    </div>
  );
}

function AgentAvailabilitySection({
  agent,
  onSetAgentEnabled,
}: {
  agent: AgentSettingsRecord;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
}) {
  const available = agent.enabled;
  return (
    <section className="agent-detail-section">
      <div className="settings-section-title">
        <strong>Availability</strong>
      </div>
      <label className="settings-switch agent-enabled-toggle">
        <input
          checked={agent.enabled}
          onChange={(event) => {
            onSetAgentEnabled(agent.id, event.currentTarget.checked);
          }}
          type="checkbox"
        />
        <span className="settings-switch-track" aria-hidden="true" />
        <span>{available ? "Enabled" : "Disabled"}</span>
      </label>
      <InlineNotice
        message={available ? "Agent is available to be selected and used." : "Agent is hidden from new task selection."}
      />
    </section>
  );
}

function AgentStatusPanel({
  agent,
  authPending,
}: {
  agent: AgentSettingsRecord;
  authPending: boolean;
}) {
  return (
    <section className={`agent-status-panel ${agent.status}`}>
      <StatusBadge status={agent.status} />
      <span>{authPending ? "Authentication is running. Follow any prompt opened by the Agent." : agentStatusCopy(agent)}</span>
      {agent.last_error_summary ? <InlineFailure message={agent.last_error_summary} /> : null}
    </section>
  );
}

function AgentAuthenticationSection({
  agent,
  authPending,
  onAuthenticate,
}: {
  agent: AgentSettingsRecord;
  authPending: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
}) {
  return (
    <section className="agent-detail-section agent-auth-methods">
      <div className="settings-section-title">
        <strong>Authentication</strong>
      </div>
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
    </section>
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
  const awaitingThisTerminal = method.kind === "terminal"
    && agentStatus === "authenticating"
    && authenticatingMethodId === method.id;
  const anotherMethodIsAuthenticating = agentStatus === "authenticating" && !awaitingThisTerminal;
  return (
    <div className={`agent-auth-method ${method.kind}`}>
      <div>
        <strong>{method.label}</strong>
        {method.description ? <small>{method.description}</small> : null}
      </div>
      {variables.map((variable) => (
        <label className="agent-field" key={variable.name}>
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
      <button
        disabled={authPending || anotherMethodIsAuthenticating || (method.kind === "env_var" && missingRequired)}
        type="button"
        onClick={() => onAuthenticate(agentId, method.id, method.kind === "env_var" ? values : undefined)}
      >
        <LockKeyhole size={13} />
        {awaitingThisTerminal ? "I've finished signing in" : method.label}
      </button>
    </div>
  );
}
