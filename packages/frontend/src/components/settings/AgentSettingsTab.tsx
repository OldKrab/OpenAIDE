import { useEffect, useState } from "react";
import type {
  AgentSettingsRecord,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
} from "@openaide/app-shell-contracts";
import { AgentSettingsDetail } from "./AgentSettingsDetail";
import { AgentSettingsList } from "./AgentSettingsList";
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
}: {
  agents: AgentSettingsRecord[];
  authPending: boolean;
  onAuthenticate: (agentId: string, methodId: string) => void;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onSetAgentEnabled: (agentId: string, enabled: boolean) => void;
  onUpdateCustomAgentMetadata: (params: CustomAgentMetadataUpdateParams) => void;
  deletedAgentId?: string;
  savedAgentId?: string;
}) {
  const [selectedId, setSelectedId] = useState(agents[0]?.id);
  const [confirmDeleteAgentId, setConfirmDeleteAgentId] = useState<string | undefined>();
  const [confirmReplaceAgentId, setConfirmReplaceAgentId] = useState<string | undefined>();
  const [draft, setDraft] = useState<AgentDraft | undefined>();
  const [pendingDeleteAgentId, setPendingDeleteAgentId] = useState<string | undefined>();
  const [pendingSaveAgentId, setPendingSaveAgentId] = useState<string | undefined>();
  const selectedAgent = agents.find((agent) => agent.id === selectedId) ?? agents[0];
  const selected = draft ? undefined : selectedAgent;
  const activeDraft = draft ?? (selected ? draftFromAgent(selected) : newAgentDraft());
  const isCustom = draft !== undefined || selected?.source_kind === "custom";
  const isCreating = draft?.agent_id === undefined;
  const missingRequiredLaunchFields = isCustom && (!activeDraft.label.trim() || !activeDraft.command_line.trim());

  useEffect(() => {
    if (!shouldConsumeAgentSaveAck({ savedAgentId, pendingSaveAgentId, hasDraft: draft !== undefined })) return;
    setDraft(undefined);
    setSelectedId(savedAgentId!);
    setPendingSaveAgentId(undefined);
  }, [draft, pendingSaveAgentId, savedAgentId]);

  useEffect(() => {
    if (!shouldConsumeAgentDeleteAck({ deletedAgentId, pendingDeleteAgentId })) return;
    setDraft(undefined);
    setSelectedId(agents.find((agent) => agent.id !== deletedAgentId)?.id ?? agents[0]?.id ?? "");
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

  return (
    <div className="settings-panel agents-settings-panel">
      <div className="agent-settings-layout">
        <AgentSettingsList
          agents={agents}
          draftActive={draft !== undefined}
          onAdd={() => setDraft(newAgentDraft())}
          onSelectAgent={selectAgent}
          selectedId={activeDraft.agent_id ?? selected?.id}
        />
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
