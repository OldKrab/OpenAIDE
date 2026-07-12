import { describe, expect, it } from "vitest";

import { composerAvailability, composerCanSubmit } from "./composerAvailability";

describe("composer availability", () => {
  it("allows a ready Task to send text or valid attachments, but never an empty message", () => {
    const base = {
      connectionStatus: "ready" as const,
      contextReady: true,
      sendCapability: { state: "ready" as const },
      attachmentsReady: true,
      allowEditingWhileSendBlocked: true,
      readyPlaceholder: "Send follow-up",
    };

    const availability = composerAvailability(base);
    expect(composerCanSubmit(availability, "Hello", 0)).toBe(true);
    expect(composerCanSubmit(availability, "", 1)).toBe(true);
    expect(composerCanSubmit(availability, "", 0)).toBe(false);
  });

  it("uses authoritative readiness and rejects unresolved attachments", () => {
    const base = {
      connectionStatus: "ready" as const,
      contextReady: true,
      allowEditingWhileSendBlocked: false,
      readyPlaceholder: "Describe the task.",
    };

    expect(composerAvailability({
      ...base,
      attachmentsReady: true,
      sendCapability: { state: "loading" as const },
    })).toMatchObject({ canEdit: false, submissionAllowed: false });
    expect(composerAvailability({
      ...base,
      attachmentsReady: false,
      sendCapability: { state: "ready" as const },
    }).submissionAllowed).toBe(false);
  });

  it("keeps an existing Task draft editable while authoritative sending is blocked", () => {
    expect(composerAvailability({
      allowEditingWhileSendBlocked: true,
      attachmentsReady: true,
      blockedPlaceholder: "Send a follow-up",
      connectionStatus: "ready",
      contextReady: true,
      readyPlaceholder: "Send follow-up",
      sendCapability: { state: "blocked" },
    })).toMatchObject({
      canEdit: true,
      submissionAllowed: false,
      placeholder: "Send a follow-up",
    });
  });

  it("disables archived Tasks and permits only the existing exact-retry state", () => {
    const base = {
      allowEditingWhileSendBlocked: true,
      attachmentsReady: true,
      connectionStatus: "ready" as const,
      contextReady: true,
      readyPlaceholder: "Send follow-up",
      sendCapability: { state: "blocked" as const },
    };

    expect(composerAvailability({ ...base, archived: true })).toMatchObject({
      canEdit: false,
      submissionAllowed: false,
      placeholder: "Restore task to send follow-up.",
    });
    expect(composerAvailability({ ...base, uncertain: true })).toMatchObject({
      canEdit: false,
      submissionAllowed: true,
      placeholder: "Retry this exact message.",
    });
  });
});
