import { AppServerProtocolError } from "@openaide/app-server-client";

/** Identifies a resolver handle that cannot be reused after attachment runtime continuity was lost. */
export function isInvalidAttachmentHandleError(error: unknown): error is AppServerProtocolError {
  return error instanceof AppServerProtocolError
    && error.protocolError.code === "attachmentHandleInvalid";
}
