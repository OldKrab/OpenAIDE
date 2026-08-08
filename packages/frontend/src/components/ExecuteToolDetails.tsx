import { Check, X } from "lucide-react";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { executeDetailInfo } from "../state/toolDetailsViewModel";

export function ExecuteToolDetails({
  details,
  fallbackPreview,
  showEmptyOutput = false,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  showEmptyOutput?: boolean;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const info = executeDetailInfo(details, step, fallbackPreview);
  return (
    <div className={`activity-tool-details activity-tool-execute-detail ${info.mode}`}>
      <code className="execute-command-chip">&gt;_ {info.command}</code>
      {info.mode === "running" ? <p className="execute-running">Running...</p> : null}
      {info.outputText ? (
        <section className={`execute-output ${info.outputTone}`}>
          <span>{info.outputLabel}</span>
          <pre>{info.outputText}</pre>
        </section>
      ) : showEmptyOutput && info.mode !== "running" ? <p className="activity-tool-muted">No output returned.</p> : null}
      {info.mode !== "running" ? (
        <p className={`execute-result ${info.failed ? "failed" : "completed"}`}>
          {info.failed ? <X size={14} aria-hidden="true" /> : <Check size={14} aria-hidden="true" />}
          <span>
            {info.failed ? "Failed" : "Completed"}
            {info.duration ? ` in ${info.duration}` : ""}
          </span>
          {info.exitCode !== undefined ? <code>exit {info.exitCode}</code> : null}
        </p>
      ) : null}
    </div>
  );
}
