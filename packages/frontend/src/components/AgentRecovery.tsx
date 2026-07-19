import { CircleAlert } from "lucide-react";
import { useState } from "react";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AgentOption } from "../state/composerOptions";

export const NODE_JS_DOWNLOAD_URL = "https://nodejs.org/en/download";
export const CODEX_SETUP_HELP_URL = "https://github.com/OldKrab/OpenAIDE/blob/main/docs/codex-setup.md";

export type AgentRecoveryKind =
  | "nodeJsRequired"
  | "authRequired"
  | "setupRequired"
  | "launchFailed";

export type AgentRecoveryActions = {
  onOpenAgentSettings: (agentId: string, returnToNewTask?: boolean) => void;
  onOpenExternal: (url: string) => void;
  onRetry: (agentId: string) => Promise<boolean>;
  onReload?: () => void;
};

export function agentRecoveryKind(
  agent: AgentOption | undefined,
  preparation?: TaskSnapshot["preparation"],
): AgentRecoveryKind | undefined {
  if (preparation?.kind === "blocked") {
    if (preparation.blocker.kind === "nodeJsRequired") return "nodeJsRequired";
    if (preparation.blocker.kind === "authRequired") return "authRequired";
    if (preparation.blocker.kind === "setupRequired") return "setupRequired";
  }
  if (preparation?.kind === "failed") return "launchFailed";
  if (agent?.status === "authRequired") return "authRequired";
  if (agent?.status === "setupRequired") {
    return agent.setupReason === "nodeJsRequired" ? "nodeJsRequired" : "setupRequired";
  }
  if (agent?.status === "failed") return "launchFailed";
  return undefined;
}

/** Builds the stable recovery identity used by a saved Task composer. */
export function taskAgentRecovery(
  agentId: string,
  agentLabel: string,
  agents: AgentOption[] | undefined,
  preparation: TaskSnapshot["preparation"],
) {
  const agent = agents?.find((candidate) => candidate.id === agentId)
    ?? { id: agentId, label: agentLabel, description: "", icon: "bot" };
  const kind = agentRecoveryKind(agent, preparation);
  return kind ? { agent, kind } : undefined;
}

export function AgentRecoveryPanel({
  actions,
  agent,
  kind,
  returnToNewTask = false,
}: {
  actions: AgentRecoveryActions;
  agent: Pick<AgentOption, "id" | "label">;
  kind: AgentRecoveryKind;
  returnToNewTask?: boolean;
}) {
  const content = recoveryContent(kind, agent.label);
  return (
    <section className={`agent-recovery-panel ${kind}`} aria-label={content.title} role="status">
      <CircleAlert aria-hidden="true" size={16} />
      <div className="agent-recovery-copy">
        <strong>{content.title}</strong>
        <small>{content.description}</small>
        <AgentRecoveryButtons
          actions={actions}
          agent={agent}
          kind={kind}
          returnToNewTask={returnToNewTask}
        />
      </div>
    </section>
  );
}

/** Renders shared recovery controls without imposing a surrounding surface. */
export function AgentRecoveryButtons({
  actions,
  agent,
  kind,
  returnToNewTask = false,
  surface = "task",
}: {
  actions: AgentRecoveryActions;
  agent: Pick<AgentOption, "id" | "label">;
  kind: AgentRecoveryKind;
  returnToNewTask?: boolean;
  surface?: "settings" | "task";
}) {
  const [checking, setChecking] = useState(false);
  const [showReload, setShowReload] = useState(false);
  const retry = async () => {
    setChecking(true);
    try {
      const ready = await actions.onRetry(agent.id);
      setShowReload(!ready && kind === "nodeJsRequired");
    } finally {
      setChecking(false);
    }
  };
  return (
    <div className="agent-recovery-actions">
      {kind === "nodeJsRequired" ? (
        <>
          <button type="button" onClick={() => actions.onOpenExternal(NODE_JS_DOWNLOAD_URL)}>Install Node.js</button>
          <button disabled={checking} type="button" onClick={() => void retry()}>{checking ? "Checking" : "Check again"}</button>
          <button type="button" onClick={() => actions.onOpenExternal(CODEX_SETUP_HELP_URL)}>Setup help</button>
          {showReload && actions.onReload ? <button type="button" onClick={actions.onReload}>Reload VS Code</button> : null}
        </>
      ) : kind === "authRequired" ? (
        <button type="button" onClick={() => actions.onOpenAgentSettings(agent.id, returnToNewTask)}>Choose sign-in method</button>
      ) : (
        <>
          <button disabled={checking} type="button" onClick={() => void retry()}>{checking ? "Trying again" : "Try again"}</button>
          {surface === "settings" && agent.id === "codex" ? (
            <button type="button" onClick={() => actions.onOpenExternal(CODEX_SETUP_HELP_URL)}>Setup help</button>
          ) : surface === "task" ? (
            <button type="button" onClick={() => actions.onOpenAgentSettings(agent.id, returnToNewTask)}>Open Agent settings</button>
          ) : null}
        </>
      )}
    </div>
  );
}

function recoveryContent(kind: AgentRecoveryKind, agentLabel: string) {
  switch (kind) {
    case "nodeJsRequired":
      return {
        title: `${agentLabel} needs Node.js`,
        description: `OpenAIDE can't access the Node.js tools required to start ${agentLabel}.`,
      };
    case "authRequired":
      return {
        title: `Sign in to use ${agentLabel}`,
        description: `${agentLabel} needs authentication before it can start the Task.`,
      };
    case "setupRequired":
      return {
        title: `Set up ${agentLabel}`,
        description: `${agentLabel} needs setup before it can start work.`,
      };
    case "launchFailed":
      return {
        title: `Couldn't start ${agentLabel}`,
        description: "OpenAIDE couldn't start the Agent. Try again or review Agent settings.",
      };
  }
}
