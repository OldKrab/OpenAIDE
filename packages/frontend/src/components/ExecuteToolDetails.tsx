import { Check, LoaderCircle, X } from "lucide-react";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { executeDetailInfo } from "../state/toolDetailsViewModel";

export function ExecuteToolDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const info = executeDetailInfo(details, step, fallbackPreview);
  return (
    <div className={`activity-tool-details activity-tool-execute-detail ${info.mode}`}>
      <code className="execute-command-chip">&gt;_ {info.command}</code>
      {info.outputs.map((output) => (
        <section className={`execute-output ${output.tone}`} key={output.label}>
          <span>{output.label}</span>
          <pre>{output.text}</pre>
        </section>
      ))}
      <p className={`execute-result ${info.mode}`}>
        {info.mode === "running"
          ? <LoaderCircle size={14} aria-hidden="true" />
          : info.mode === "completed"
            ? <Check size={14} aria-hidden="true" />
            : <X size={14} aria-hidden="true" />}
        <span>
          {info.resultLabel}
          {info.duration ? ` in ${info.duration}` : ""}
        </span>
        {info.exitCode !== undefined ? <code>exit {info.exitCode}</code> : null}
      </p>
    </div>
  );
}
