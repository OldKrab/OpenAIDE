import { useCallback, useEffect, useRef, useState } from "react";
import { postHostMessage } from "../services/hostBridge";
import { sendWebviewTelemetry } from "../state/hostMessageTelemetry";

/** Why a Send held for in-flight uploads stopped waiting. */
export type SendAfterUploadsOutcome =
  | "sent"
  | "edited"
  | "cancelled"
  | "upload_failed"
  | "blocked"
  | "unmounted";

export type SendAfterUploadsReport = (event: string, fields: Record<string, string | number>) => void;

const reportToHost: SendAfterUploadsReport = (event, fields) => {
  // Diagnostics must never block or drop the user's Send, including before an App Shell is installed.
  try {
    sendWebviewTelemetry(postHostMessage, event, fields);
  } catch {
    // Nothing to report through.
  }
};

/**
 * Holds a Send the user asked for while draft attachments were still uploading
 * (for example, Enter pressed together with an Image paste) and performs it
 * once every upload has finished. Send still waits for all uploads; this only
 * keeps the user's intent across that wait instead of dropping it.
 *
 * The held Send is ephemeral Composer presentation: it lives only in this
 * mounted Composer and reaches App Server as the normal Send. Any change to the
 * draft, an upload failure, or a Composer that can no longer send ends it
 * without sending, so the user never sends a draft different from the one they
 * confirmed.
 */
export function useSendAfterUploads({
  canSubmit,
  failedUploadIds,
  report = reportToHost,
  submit,
  uploadsInFlight,
}: {
  /** Full Send readiness, evaluated once no upload is in flight. */
  canSubmit: boolean;
  /** Upload rows currently in the failed state; a new one ends the held Send. */
  failedUploadIds: readonly string[];
  report?: SendAfterUploadsReport;
  submit: () => void;
  uploadsInFlight: number;
}) {
  const [waiting, setWaiting] = useState(false);
  const heldRef = useRef<{ startedAt: number; knownFailures: ReadonlySet<string> } | undefined>(undefined);
  const latest = useRef({ failedUploadIds, report, submit, uploadsInFlight });
  latest.current = { failedUploadIds, report, submit, uploadsInFlight };

  const end = useCallback((outcome: SendAfterUploadsOutcome) => {
    const held = heldRef.current;
    if (!held) return;
    heldRef.current = undefined;
    setWaiting(false);
    latest.current.report("composer_send_after_uploads_end", {
      outcome,
      duration_ms: Math.round(performance.now() - held.startedAt),
      uploads_in_flight: latest.current.uploadsInFlight,
    });
  }, []);

  const hold = useCallback(() => {
    if (heldRef.current) return;
    heldRef.current = { startedAt: performance.now(), knownFailures: new Set(latest.current.failedUploadIds) };
    setWaiting(true);
    latest.current.report("composer_send_after_uploads_start", {
      uploads_in_flight: latest.current.uploadsInFlight,
    });
  }, []);

  const failedKey = failedUploadIds.join("\n");
  useEffect(() => {
    const held = heldRef.current;
    if (!held) return;
    // A row that failed before the user pressed Send is already visible and
    // does not block Send; only a failure that happens during the wait ends it.
    if (failedUploadIds.some((id) => !held.knownFailures.has(id))) {
      end("upload_failed");
      return;
    }
    held.knownFailures = new Set(failedUploadIds);
    if (uploadsInFlight > 0) return;
    if (!canSubmit) {
      end("blocked");
      return;
    }
    end("sent");
    latest.current.submit();
    // failedKey stands in for the array identity, which changes every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canSubmit, end, failedKey, uploadsInFlight, waiting]);

  useEffect(() => () => end("unmounted"), [end]);

  return { cancel: end, hold, waiting };
}
