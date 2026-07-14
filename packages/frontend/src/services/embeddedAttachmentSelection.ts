import {
  ATTACHMENT_CONFIRM_EMBEDDED,
  ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
  type BackendConnection,
  type FileBrowserEntryId,
  type TaskId,
} from "@openaide/app-server-client";
import {
  attachmentCandidateResource,
  releaseAttachmentResources,
} from "./attachmentResources";

/** Owns a non-sendable candidate until confirmation consumes it or cleanup releases it. */
export async function createConfirmedEmbeddedAttachment(
  backendConnection: Pick<BackendConnection, "request">,
  taskId: TaskId,
  entryId: FileBrowserEntryId,
  candidateDisposition: () => "current" | "release" | "forget" = () => "current",
) {
  const candidate = await backendConnection.request(ATTACHMENT_CREATE_EMBEDDED_CANDIDATE, {
    taskId,
    entryId,
  });
  const candidateId = candidate.candidate.candidateId;
  const initialDisposition = candidateDisposition();
  if (initialDisposition !== "current") {
    if (initialDisposition === "release") {
      releaseAttachmentResources(backendConnection, taskId, [attachmentCandidateResource(candidateId)]);
    }
    throw new Error("Attachment selection was superseded before confirmation.");
  }
  try {
    const confirmed = await backendConnection.request(ATTACHMENT_CONFIRM_EMBEDDED, {
      taskId,
      candidates: [candidateId],
    });
    const error = confirmed.errors[0];
    if (error) throw new Error(error.message);
    const attachment = confirmed.attachments[0];
    if (!attachment) throw new Error("Embedded attachment was not confirmed.");
    return attachment;
  } catch (error) {
    if (candidateDisposition() !== "forget") {
      releaseAttachmentResources(backendConnection, taskId, [attachmentCandidateResource(candidateId)]);
    }
    throw error;
  }
}
