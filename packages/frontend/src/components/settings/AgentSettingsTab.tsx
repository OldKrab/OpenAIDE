import { ArrowLeft } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  AgentSettingsRecord,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
} from "@openaide/app-shell-contracts";
import { AgentSettingsDetail } from "./AgentSettingsDetail";
import { AgentSettingsList } from "./AgentSettingsList";
import type { AgentRecoveryActions } from "../AgentRecovery";
import {
  draftFromAgent,
  newAgentDraft,
  shouldConsumeAgentDeleteAck,
  shouldConsumeAgentSaveAck,
  draftChangesLaunch,
  type AgentDraft,
} from "./agentSettingsModel";

export { shouldConsumeAgentDeleteAck, shouldConsumeAgentSaveAck } from "./agentSettingsModel";

export function AgentSettingsTab({
  agents,
  authPending,
  onAuthenticate,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onReplaceCustomAgent,
  onSetAgentEnabled,
  onUpdateCustomAgentMetadata,
  deletedAgentId,
  savedAgentId,
  preferredAgentId,
  recoveryActions,
}: {
  agents: AgentSettingsRecord[];
  authPending: boolean;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void | Promise<boolean>;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  deletedAgentId?: string;
  savedAgentId?: string;
  preferredAgentId?: string;
  recoveryActions?: AgentRecoveryActions;
}) {
  const [selectedId, setSelectedId] = useState<string>();
  const [confirmDeleteAgentId, setConfirmDeleteAgentId] = useState<string | undefined>();
  const [confirmReplaceAgentId, setConfirmReplaceAgentId] = useState<string | undefined>();
  const [draft, setDraft] = useState<AgentDraft | undefined>();
  const [pendingDeleteAgentId, setPendingDeleteAgentId] = useState<string | undefined>();
  const [pendingSaveAgentId, setPendingSaveAgentId] = useState<string | undefined>();
  const selectedAgent = agents.find((agent) => agent.id === selectedId);
  const selected = draft ? undefined : selectedAgent;
  const activeDraft = draft ?? (selected ? draftFromAgent(selected) : newAgentDraft());
  const isCustom = draft !== undefined || selected?.source_kind === "custom";
  const isCreating = draft?.agent_id === undefined;
  const missingRequiredLaunchFields = isCustom && (!activeDraft.label.trim() || !activeDraft.command_line.trim());

  useEffect(() => {
    if (preferredAgentId && agents.some((agent) => agent.id === preferredAgentId)) {
      setSelectedId(preferredAgentId);
    }
  }, [agents, preferredAgentId]);

  useEffect(() => {
    if (!shouldConsumeAgentSaveAck({ savedAgentId, pendingSaveAgentId, hasDraft: draft !== undefined })) return;
    setDraft(undefined);
    setSelectedId(savedAgentId!);
    setPendingSaveAgentId(undefined);
  }, [draft, pendingSaveAgentId, savedAgentId]);

  useEffect(() => {
    if (!shouldConsumeAgentDeleteAck({ deletedAgentId, pendingDeleteAgentId })) return;
    setDraft(undefined);
    setSelectedId(undefined);
    setPendingDeleteAgentId(undefined);
  }, [agents, deletedAgentId, pendingDeleteAgentId]);

  const selectAgent = (agent: AgentSettingsRecord) => {
    if (draft) return;
    setConfirmDeleteAgentId(undefined);
    setConfirmReplaceAgentId(undefined);
    setDraft(undefined);
    setSelectedId(agent.id);
  };
  const updateDraft = (patch: Partial<AgentDraft>) => {
    setConfirmReplaceAgentId(undefined);
    setDraft({ ...activeDraft, ...patch });
  };
  const saveDraft = () => {
    setConfirmDeleteAgentId(undefined);
    if (missingRequiredLaunchFields) return;
    const replacingLaunch = selectedAgent?.source_kind === "custom" && draftChangesLaunch(selectedAgent, activeDraft);
    if (replacingLaunch && confirmReplaceAgentId !== activeDraft.agent_id) {
      setConfirmReplaceAgentId(activeDraft.agent_id);
      return;
    }
    setConfirmReplaceAgentId(undefined);
    setPendingSaveAgentId(activeDraft.agent_id ?? "__new__");
    if (!activeDraft.agent_id) {
      onCreateCustomAgent(customAgentCreateParams(activeDraft));
      return;
    }
    if (replacingLaunch) {
      onReplaceCustomAgent({
        ...customAgentCreateParams(activeDraft),
        source_agent_id: activeDraft.agent_id,
        confirmed: true,
      });
      return;
    }
    onUpdateCustomAgentMetadata({
      agent_id: activeDraft.agent_id,
      label: activeDraft.label,
      icon: activeDraft.icon,
      enabled: activeDraft.enabled,
    });
  };
  const deleteDraft = () => {
    if (!activeDraft.agent_id) return;
    if (confirmDeleteAgentId !== activeDraft.agent_id) {
      setConfirmDeleteAgentId(activeDraft.agent_id);
      return;
    }
    setConfirmDeleteAgentId(undefined);
    setPendingDeleteAgentId(activeDraft.agent_id);
    onDeleteCustomAgent(activeDraft.agent_id);
  };
  const cancelDraft = () => {
    setConfirmDeleteAgentId(undefined);
    setConfirmReplaceAgentId(undefined);
    setPendingSaveAgentId(undefined);
    setDraft(undefined);
  };

  if (!selected && !draft) {
    return (
      <div className="settings-panel agents-settings-panel">
        <AgentSettingsList
          agents={agents}
          onAdd={() => setDraft(newAgentDraft())}
          onSelectAgent={selectAgent}
        />
      </div>
    );
  }

  return (
    <div className="settings-panel agents-settings-panel">
      <div className="agent-focused-view">
        <button
          aria-label="Back to Agents"
          className="settings-detail-back agent-detail-back"
          disabled={draft !== undefined && !isCreating}
          onClick={() => setSelectedId(undefined)}
          title={draft !== undefined && !isCreating ? "Save or cancel changes first" : undefined}
          type="button"
        >
          <ArrowLeft size={14} /><span>Back to Agents</span>
        </button>
        <AgentSettingsDetail
          activeDraft={activeDraft}
          authPending={authPending}
          confirmDeleteAgentId={confirmDeleteAgentId}
          confirmReplaceAgentId={confirmReplaceAgentId}
          isCreating={isCreating}
          isCustom={isCustom}
          onAuthenticate={onAuthenticate}
          onCancelDraft={draft !== undefined ? cancelDraft : undefined}
          onDeleteClick={deleteDraft}
          onSaveDraft={saveDraft}
          saveBlockedMessage={missingRequiredLaunchFields ? "Name and command are required." : undefined}
          onSetAgentEnabled={onSetAgentEnabled}
          onUpdateDraft={updateDraft}
          recoveryActions={recoveryActions}
          selected={selected}
        />
      </div>
    </div>
  );
}

function customAgentCreateParams(draft: AgentDraft): CustomAgentCreateParams {
  return {
    label: draft.label,
    icon: draft.icon,
    command_line: draft.command_line,
    enabled: draft.enabled,
    env: draft.env,
  };
}
