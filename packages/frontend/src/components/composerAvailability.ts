import { hasComposerContent } from "./composerDraftPolicy";

type SendCapability = {
  state: "loading" | "ready" | "blocked" | "failed";
  blockers?: Array<{ message: string }>;
};

/** Complete render policy consumed by every Task Composer surface. */
export type ComposerAvailability = {
  canEdit: boolean;
  submissionAllowed: boolean;
  placeholder: string;
  submitting: boolean;
  submitActionLabel: string;
  submitPendingLabel: string;
};

/** Combines authoritative Task readiness with client-local connection and attachment facts. */
export function composerAvailability({
  allowEditingWhileSendBlocked,
  archived = false,
  attachmentsReady,
  blockedPlaceholder,
  connectionStatus,
  contextPlaceholder = "Preparing task.",
  contextReady,
  readyPlaceholder,
  sendCapability,
  submitActionLabel = "Send message",
  submitPendingLabel = "Sending message",
  submitting = false,
  uncertain = false,
}: {
  allowEditingWhileSendBlocked: boolean;
  archived?: boolean;
  attachmentsReady: boolean;
  blockedPlaceholder?: string;
  connectionStatus: "connecting" | "ready" | "reconnecting" | "unavailable";
  contextPlaceholder?: string;
  contextReady: boolean;
  readyPlaceholder: string;
  sendCapability?: SendCapability;
  submitActionLabel?: string;
  submitPendingLabel?: string;
  submitting?: boolean;
  uncertain?: boolean;
}): ComposerAvailability {
  if (archived) {
    return unavailable("Restore task to send follow-up.");
  }
  if (submitting) {
    return {
      ...unavailable("Sending."),
      submitting: true,
      submitActionLabel,
      submitPendingLabel,
    };
  }
  if (!contextReady) {
    return unavailable(contextPlaceholder);
  }
  if (connectionStatus !== "ready") {
    const canEdit = allowEditingWhileSendBlocked
      && (connectionStatus === "reconnecting" || connectionStatus === "unavailable");
    return {
      ...unavailable(canEdit ? "Reconnecting. Draft is saved here." : "Connecting to App Server."),
      canEdit,
    };
  }
  if (uncertain) {
    return {
      canEdit: false,
      submissionAllowed: true,
      placeholder: "Retry this exact message.",
      submitting: false,
      submitActionLabel,
      submitPendingLabel,
    };
  }
  if (sendCapability?.state !== "ready") {
    return {
      ...unavailable(
        blockedPlaceholder
          ?? sendCapability?.blockers?.[0]?.message
          ?? "Preparing task.",
      ),
      canEdit: allowEditingWhileSendBlocked,
    };
  }

  return {
    canEdit: true,
    submissionAllowed: attachmentsReady,
    placeholder: readyPlaceholder,
    submitting: false,
    submitActionLabel,
    submitPendingLabel,
  };

  function unavailable(placeholder: string): ComposerAvailability {
    return {
      canEdit: false,
      submissionAllowed: false,
      placeholder,
      submitting: false,
      submitActionLabel,
      submitPendingLabel,
    };
  }
}

/** Applies the live editor contents to the already-resolved availability model. */
export function composerCanSubmit(
  availability: ComposerAvailability,
  prompt: string,
  attachmentCount: number,
) {
  return availability.submissionAllowed && hasComposerContent(prompt, attachmentCount);
}
