import { Bot, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  AgentSettingsRecord,
  CustomAgentCreateParams,
  CustomAgentMetadataUpdateParams,
  CustomAgentReplaceParams,
} from "@openaide/app-shell-contracts";
import type { AgentDisableOutcome } from "../../intents/agentSettingsIntents";
import { currentFrontendShell } from "../../services/frontendShell";
import { AgentDisableDialog } from "../AgentDisableDialog";
import { AgentIcon } from "../AgentIcon";
import type { AgentRecoveryActions } from "../AgentRecovery";
import { useAgentDisableConfirmation } from "../useAgentDisableConfirmation";
import { AgentPage, agentStatus } from "./AgentPage";
import {
  draftChangesLaunch,
  draftFromAgent,
  newAgentDraft,
  shouldConsumeAgentDeleteAck,
  shouldConsumeAgentSaveAck,
  type AgentDraft,
} from "./agentSettingsModel";
import { settingsInfo } from "./settingsInfo";
import { SettingsAvatar, SettingsBlankState, SettingsButton, SettingsConfirm, SettingsSwitch } from "./settingsKit";
import type { SettingsRow, SettingsSection } from "./settingsModel";

export type AgentsSectionProps = {
  /** Undefined while the first Agent list is loading. */
  agents?: AgentSettingsRecord[];
  available: boolean;
  deletedAgentId?: string;
  onAuthenticate: (agentId: string, methodId: string, values?: Record<string, string>) => void | Promise<boolean>;
  onCancelAuthentication?: (agentId: string) => void | Promise<void>;
  onCreateCustomAgent: (params: CustomAgentCreateParams) => void;
  onDeleteCustomAgent: (agentId: string) => void;
  onLogout?: (agentId: string) => boolean | void | Promise<boolean | void>;
  onReplaceCustomAgent: (params: CustomAgentReplaceParams) => void;
  onSetAgentEnabled: (
    agentId: string,
    enabled: boolean,
    acceptedActiveWorkInterruption?: boolean,
  ) => Promise<AgentDisableOutcome>;
  onUpdateCustomAgentMetadata: (
    params: CustomAgentMetadataUpdateParams,
    acceptedActiveWorkInterruption?: boolean,
  ) => Promise<AgentDisableOutcome>;
  preferredAgentId?: string;
  recoveryActions?: AgentRecoveryActions;
  saveError?: string;
  savedAgentId?: string;
};

type Confirm = "delete" | "replace" | "discard";

/**
 * The Agents available for tasks and the page of each one. The list and every
 * mutation outcome are App Server state; this owns only the open page, the
 * unsaved draft, and the questions asked before a destructive step.
 */
export function useAgentsSection({
  agents,
  available,
  deletedAgentId,
  onAuthenticate,
  onCancelAuthentication,
  onCreateCustomAgent,
  onDeleteCustomAgent,
  onLogout,
  onReplaceCustomAgent,
  onSetAgentEnabled,
  onUpdateCustomAgentMetadata,
  preferredAgentId,
  recoveryActions,
  saveError,
  savedAgentId,
}: AgentsSectionProps): SettingsSection | undefined {
  const [selectedId, setSelectedId] = useState<string>();
  const [draft, setDraft] = useState<AgentDraft>();
  const [confirm, setConfirm] = useState<Confirm>();
  const [pendingDeleteAgentId, setPendingDeleteAgentId] = useState<string>();
  const [pendingSaveAgentId, setPendingSaveAgentId] = useState<string>();
  const { cancelDisable, confirmDisable, pendingDisable, requestDisable } = useAgentDisableConfirmation();
  const list = agents ?? [];
  const selectedAgent = list.find((agent) => agent.id === selectedId);
  const isCreating = draft !== undefined && draft.agent_id === undefined;
  const activeDraft = draft ?? (selectedAgent ? draftFromAgent(selectedAgent) : newAgentDraft());
  const replacingLaunch = Boolean(draft && selectedAgent?.source_kind === "custom" && draftChangesLaunch(selectedAgent, activeDraft));
  const missingRequiredLaunchFields = Boolean(draft) && (!activeDraft.label.trim() || !activeDraft.command_line.trim());
  const replaceAgentRoute = (agentId?: string) => currentFrontendShell()?.navigation?.replaceSettingsAgent?.(agentId);

  useEffect(() => {
    if (preferredAgentId && agents?.some((agent) => agent.id === preferredAgentId)) setSelectedId(preferredAgentId);
  }, [agents, preferredAgentId]);

  useEffect(() => {
    if (!shouldConsumeAgentSaveAck({
      savedAgentId,
      removedAgentId: deletedAgentId,
      pendingSaveAgentId,
      hasDraft: draft !== undefined,
    })) return;
    const createdAgent = pendingSaveAgentId === "__new__";
    setDraft(undefined);
    setPendingSaveAgentId(undefined);
    if (createdAgent) {
      // Land on the Agent list so the new row is the visible result of Save while
      // its first process check runs. Staying on the page looks like no change.
      setSelectedId(undefined);
      replaceAgentRoute();
      return;
    }
    setSelectedId(savedAgentId!);
  }, [deletedAgentId, draft, pendingSaveAgentId, savedAgentId]);

  useEffect(() => {
    // Failed mutations have no save acknowledgement, so release the local
    // pending state when Settings exposes the operation error.
    if (saveError && pendingSaveAgentId) setPendingSaveAgentId(undefined);
  }, [pendingSaveAgentId, saveError]);

  useEffect(() => {
    if (!shouldConsumeAgentDeleteAck({ deletedAgentId, pendingDeleteAgentId })) return;
    setDraft(undefined);
    setSelectedId(undefined);
    setPendingDeleteAgentId(undefined);
  }, [agents, deletedAgentId, pendingDeleteAgentId]);

  if (!available) return undefined;

  const leaveDetail = () => {
    setConfirm(undefined);
    setPendingSaveAgentId(undefined);
    setDraft(undefined);
    setSelectedId(undefined);
    replaceAgentRoute();
  };
  const closeDetail = () => {
    // An unsaved draft is never dropped by navigation alone (ADR 0021).
    if (draft) {
      setConfirm("discard");
      return false;
    }
    leaveDetail();
    return true;
  };
  const requestEnabled = (agentId: string, enabled: boolean) => {
    if (enabled) {
      void onSetAgentEnabled(agentId, true);
      return;
    }
    requestDisable(agentId, (acceptedActiveWorkInterruption) =>
      onSetAgentEnabled(agentId, false, acceptedActiveWorkInterruption));
  };
  const send = () => {
    setPendingSaveAgentId(activeDraft.agent_id ?? "__new__");
    if (!activeDraft.agent_id) {
      onCreateCustomAgent(customAgentCreateParams(activeDraft));
      return;
    }
    if (replacingLaunch) {
      onReplaceCustomAgent({ ...customAgentCreateParams(activeDraft), source_agent_id: activeDraft.agent_id, confirmed: true });
      return;
    }
    const agentId = activeDraft.agent_id;
    const metadata = { agent_id: agentId, label: activeDraft.label, icon: activeDraft.icon };
    if (activeDraft.enabled) {
      void onUpdateCustomAgentMetadata({ ...metadata, enabled: true });
      return;
    }
    // Availability is part of the same save, so switching it off needs the same
    // acknowledgement as the toggle instead of becoming a second way to disable.
    requestDisable(
      agentId,
      (acceptedActiveWorkInterruption) => onUpdateCustomAgentMetadata({ ...metadata, enabled: false }, acceptedActiveWorkInterruption),
      () => setPendingSaveAgentId(undefined),
    );
  };
  const save = () => {
    if (missingRequiredLaunchFields) return;
    if (replacingLaunch) setConfirm("replace");
    else send();
  };

  const row = (agent: AgentSettingsRecord): SettingsRow => ({
    id: agent.id,
    label: agent.label,
    description: agent.description || undefined,
    keywords: `${agent.source_kind === "custom" ? "custom" : "built-in"} agent ${agent.command_line ?? ""}`,
    leading: <SettingsAvatar><AgentIcon agentId={agent.id} agentName={agent.label} icon={agent.icon} size={17} /></SettingsAvatar>,
    status: agent.enabled ? agentStatus(agent) : { tone: "off", text: "Off" },
    control: <SettingsSwitch checked={agent.enabled} label={`${agent.label} available`} onChange={(enabled) => requestEnabled(agent.id, enabled)} />,
    onOpen: () => {
      setSelectedId(agent.id);
      replaceAgentRoute(agent.id);
    },
  });
  const pendingDisableAgent = list.find((agent) => agent.id === pendingDisable?.agentId);
  const showDetail = Boolean(draft || selectedAgent);

  return {
    id: "agents",
    label: "Agents",
    icon: Bot,
    info: settingsInfo.agents,
    action: agents ? (
      <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={() => setDraft(newAgentDraft())} variant="primary">Add agent</SettingsButton>
    ) : undefined,
    notice: agents ? undefined : <SettingsBlankState title="Loading agents…" />,
    groups: agents ? [
      { id: "built-in", title: "Built-in", rows: list.filter((agent) => agent.source_kind === "built_in").map(row) },
      { id: "custom", title: "Custom", empty: "None.", rows: list.filter((agent) => agent.source_kind === "custom").map(row) },
    ].filter((group) => group.rows.length || group.empty) : [],
    closeDetail: showDetail ? closeDetail : undefined,
    detail: showDetail ? (
      <AgentPage
        dirty={draft !== undefined}
        draft={activeDraft}
        isCreating={isCreating}
        onAuthenticate={onAuthenticate}
        onBack={closeDetail}
        onCancelAuthentication={onCancelAuthentication}
        onCancelDraft={isCreating ? leaveDetail : () => { setDraft(undefined); setPendingSaveAgentId(undefined); }}
        onDelete={() => setConfirm("delete")}
        onLogout={onLogout}
        onSave={save}
        onSetAvailable={(enabled) => {
          // While a draft is open the switch is part of it; otherwise it applies at once.
          if (draft) setDraft({ ...activeDraft, enabled });
          else if (selectedAgent) requestEnabled(selectedAgent.id, enabled);
        }}
        onUpdateDraft={(patch) => setDraft({ ...activeDraft, ...patch })}
        recoveryActions={recoveryActions}
        saveBlockedMessage={missingRequiredLaunchFields ? "Name and command are required." : undefined}
        saveChecksConnection={isCreating || replacingLaunch}
        saveError={draft ? saveError : undefined}
        savePending={pendingSaveAgentId !== undefined}
        selected={isCreating ? undefined : selectedAgent}
      />
    ) : undefined,
    overlay: (
      <>
        {confirm === "discard" ? (
          <SettingsConfirm
            confirmLabel="Discard"
            detail="Your edits to this Agent are not saved."
            label="Discard changes confirmation"
            onClose={() => setConfirm(undefined)}
            onConfirm={leaveDetail}
            title="Discard changes?"
          />
        ) : null}
        {confirm === "replace" ? (
          <SettingsConfirm
            confirmLabel="Replace"
            danger={false}
            detail="Launch changes create a new Agent identity and remove this custom Agent. Earlier tasks keep their history."
            label="Replace Agent confirmation"
            onClose={() => setConfirm(undefined)}
            onConfirm={send}
            title={`Replace ${selectedAgent?.label ?? "this Agent"}?`}
          />
        ) : null}
        {confirm === "delete" && activeDraft.agent_id ? (
          <SettingsConfirm
            confirmLabel="Delete agent"
            detail="New tasks can no longer use it. This cannot be undone."
            label="Delete Agent confirmation"
            onClose={() => setConfirm(undefined)}
            onConfirm={() => {
              setPendingDeleteAgentId(activeDraft.agent_id);
              onDeleteCustomAgent(activeDraft.agent_id!);
            }}
            title={`Delete ${selectedAgent?.label ?? "this Agent"}?`}
          />
        ) : null}
        {pendingDisable && pendingDisableAgent ? (
          <AgentDisableDialog
            agentLabel={pendingDisableAgent.label}
            onCancel={cancelDisable}
            onConfirm={confirmDisable}
            runningTaskCount={pendingDisable.runningTaskCount}
          />
        ) : null}
      </>
    ),
  };
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
