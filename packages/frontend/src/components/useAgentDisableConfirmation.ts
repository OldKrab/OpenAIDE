import { useState } from "react";
import type { AgentDisableOutcome } from "../intents/agentSettingsIntents";

/**
 * Disabling an Agent stops its process, so it interrupts that Agent's running Tasks; only the
 * App Server knows which Tasks are running, so it answers an unconfirmed request with
 * `confirmation-required` and the count it owns.
 *
 * `requestDisable` sends the unconfirmed change and holds it when the acknowledgement is
 * required. The surface that showed the button renders `AgentDisableDialog` from
 * `pendingDisable`, so the confirmation appears where the user acted, and `confirmDisable`
 * repeats the exact request that was refused.
 */
export function useAgentDisableConfirmation() {
  const [pendingDisable, setPendingDisable] = useState<PendingAgentDisable>();

  const requestDisable = (
    agentId: string,
    attempt: (acceptedActiveWorkInterruption: boolean) => Promise<AgentDisableOutcome>,
    onDismiss?: () => void,
  ) => {
    void Promise.resolve(attempt(false))
      .then((outcome) => {
        if (outcome.kind !== "confirmation-required") return;
        setPendingDisable({ agentId, runningTaskCount: outcome.runningTaskCount, attempt, onDismiss });
      })
      .catch(() => undefined);
  };

  const confirmDisable = () => {
    const pending = pendingDisable;
    if (!pending) return;
    setPendingDisable(undefined);
    void Promise.resolve(pending.attempt(true)).catch(() => undefined);
  };

  const cancelDisable = () => {
    const pending = pendingDisable;
    setPendingDisable(undefined);
    // A change that belonged to a Save leaves the editor pending until it is released.
    pending?.onDismiss?.();
  };

  return { cancelDisable, confirmDisable, pendingDisable, requestDisable };
}

type PendingAgentDisable = {
  agentId: string;
  runningTaskCount: number;
  attempt: (acceptedActiveWorkInterruption: boolean) => Promise<AgentDisableOutcome>;
  onDismiss?: () => void;
};
