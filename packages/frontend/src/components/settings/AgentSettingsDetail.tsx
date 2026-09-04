import type { ReactNode } from "react";
import {
  Bot,
  CircleAlert,
  FileText,
  LoaderCircle,
  Palette,
  Save,
  Terminal,
  Trash2,
  Unplug,
  X,
} from "lucide-react";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentIcon } from "../AgentIcon";
import { CODEX_INTEGRATION_INSTALLING_LABEL } from "../agentActivityPresentation";
import { AgentRecoveryButtons, type AgentRecoveryActions, type AgentRecoveryKind } from "../AgentRecovery";
import { AgentEnvEditor, AgentIconPicker } from "./AgentCustomFields";
import type { AgentDraft } from "./agentSettingsModel";
import { agentStatusCopy } from "./agentSettingsModel";
import { AgentSignIn } from "./AgentSignIn";
import { InlineFailure, InlineNotice } from "./settingsPresentation";

export function AgentSettingsDetail({
  activeDraft,
  confirmDeleteAgentId,
  confirmReplaceAgentId,
  isCreating,
  isCustom,
  isEditing = false,
  onAuthenticate,
  onCancelAuthentication,
  onLogout,
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
  confirmDeleteAgentId?: string;
  confirmReplaceAgentId?: string;
  isCreating: boolean;
  isCustom: boolean;
  isEditing?: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void;
  onCancelAuthentication?: (agentId: string) => void | Promise<void>;
  onLogout?: (agentId: string) => boolean | void | Promise<boolean | void>;
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
          {selected && selected.status !== "disconnected" ? <AgentStatusText agent={selected} /> : null}
        </span>
        {!isCreating ? (
          <span className="agent-page-header-toggle">
            <SettingsSwitch checked={selected?.enabled ?? activeDraft.enabled} label={`${title} available`} onChange={setAvailable} />
            <small>{(selected?.enabled ?? activeDraft.enabled) ? "Available" : "Unavailable"}</small>
          </span>
        ) : null}
      </header>

      {selected && selected.auth_methods.length > 0 ? (
        <AgentSignIn agent={selected} onAuthenticate={onAuthenticate} onCancel={onCancelAuthentication} onLogout={onLogout} />
      ) : selected && needsStatusDetail(selected) ? (
        <AgentStatusSection agent={selected} recoveryActions={recoveryActions} />
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
              <button className="danger" type="button" onClick={onDeleteClick}>
                <Trash2 size={13} />
                {confirmDeleteAgentId === activeDraft.agent_id ? "Confirm delete" : "Delete"}
              </button>
            ) : null}
            {onCancelDraft ? <button type="button" onClick={onCancelDraft}><X size={13} /><span>Cancel</span></button> : null}
            <button
              aria-busy={savePending}
              className="primary"
              disabled={savePending || Boolean(saveBlockedMessage)}
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
  if (agent.status === "disconnected") return "connectionCheck";
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
  const installing = agent.status === "installing";
  return <span aria-busy={installing || undefined} className={`agent-page-status ${agent.status}`}>
    {installing ? <LoaderCircle aria-hidden="true" className="spin" size={12} /> : attention ? <CircleAlert size={12} /> : <i />}
    {shortStatus(agent.status)}
  </span>;
}

function AgentStatusSection({
  agent,
  recoveryActions,
}: {
  agent: AgentSettingsRecord;
  recoveryActions?: AgentRecoveryActions;
}) {
  const recoveryKind = agentSettingsRecoveryKind(agent);
  const starting = agent.status === "launching" || agent.status === "installing";
  const disconnected = agent.status === "disconnected";
  return (
    <AgentPageSection label={starting ? "Status" : disconnected ? "Connection" : "Setup"}>
      <div className={`agent-page-surface${starting || disconnected ? "" : " attention"}${disconnected ? " agent-connection-summary" : ""}`}>
        <AgentPageRow
          action={recoveryActions && recoveryKind ? <AgentRecoveryButtons actions={recoveryActions} agent={agent} kind={recoveryKind} surface="settings" /> : <span />}
          detail={disconnected ? undefined : agentStatusCopy(agent)}
          icon={starting
            ? <LoaderCircle aria-hidden="true" className="spin" size={16} />
            : disconnected
              ? <Unplug aria-hidden="true" size={16} />
              : <CircleAlert size={16} />}
          label={starting ? "Starting" : disconnected ? "Not connected" : "Action required"}
        />
        {agent.last_error_summary ? <InlineFailure message={agent.last_error_summary} /> : null}
      </div>
    </AgentPageSection>
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
  return needsAttention(agent)
    || agent.status === "unprobed"
    || agent.status === "disconnected"
    || agent.status === "authenticating"
    || agent.status === "launching"
    || agent.status === "installing";
}

function shortStatus(status: AgentSettingsRecord["status"]) {
  switch (status) {
    case "auth_required": return "Sign in required";
    case "setup_required": return "Finish setup";
    case "failed": return "Needs attention";
    case "disabled": return "Disabled";
    case "connected": return "Connected";
    case "ready": return "Ready";
    case "installing": return CODEX_INTEGRATION_INSTALLING_LABEL;
    case "launching": return "Starting";
    default: return status.replaceAll("_", " ");
  }
}
