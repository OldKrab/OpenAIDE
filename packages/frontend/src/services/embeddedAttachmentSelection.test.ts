import { describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_CONFIRM_EMBEDDED,
  ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
  ATTACHMENT_RELEASE,
  type BackendConnection,
} from "@openaide/app-server-client";
import { createConfirmedEmbeddedAttachment } from "./embeddedAttachmentSelection";

describe("embedded attachment selection", () => {
  it("forgets a candidate that belongs to a replaced replica", async () => {
    let disposition: "current" | "forget" = "current";
    const request = vi.fn(async (method: string) => {
      if (method !== ATTACHMENT_CREATE_EMBEDDED_CANDIDATE) throw new Error(method);
      disposition = "forget";
      return { candidate: { candidateId: "candidate_1" } };
    });

    await expect(createConfirmedEmbeddedAttachment(
      { request: request as unknown as BackendConnection["request"] },
      "task_1" as never,
      "entry_1" as never,
      () => disposition,
    )).rejects.toThrow("superseded");

    expect(request).toHaveBeenCalledOnce();
    expect(request).not.toHaveBeenCalledWith(ATTACHMENT_RELEASE, expect.anything());
  });

  it("releases a same-replica candidate after confirmation fails", async () => {
    const request = vi.fn(async (method: string) => {
      if (method === ATTACHMENT_CREATE_EMBEDDED_CANDIDATE) {
        return { candidate: { candidateId: "candidate_1" } };
      }
      if (method === ATTACHMENT_CONFIRM_EMBEDDED) throw new Error("confirmation failed");
      if (method === ATTACHMENT_RELEASE) return { outcomes: [] };
      throw new Error(method);
    });

    await expect(createConfirmedEmbeddedAttachment(
      { request: request as unknown as BackendConnection["request"] },
      "task_1" as never,
      "entry_1" as never,
    )).rejects.toThrow("confirmation failed");

    expect(request).toHaveBeenCalledWith(ATTACHMENT_RELEASE, {
      taskId: "task_1",
      resources: [{ kind: "candidate", id: "candidate_1" }],
    });
  });
});
