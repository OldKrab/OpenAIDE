import { useState } from "react";
import { ChevronDown, CircleAlert, LoaderCircle } from "lucide-react";
import type { CompactionStatus } from "@openaide/app-shell-contracts";
import { AgentMarkdown } from "./AgentMarkdown";

const STATUS_LABEL: Record<CompactionStatus, string> = {
  in_progress: "Compacting context…",
  completed: "Context compacted",
  failed: "Context compaction failed",
  cancelled: "Context compaction cancelled",
  unknown: "Context compaction",
};

/**
 * Chat row for an Agent-owned context compaction. The Agent's summary is model-facing, so it stays
 * collapsed behind the divider and is only readable on request.
 */
export function CompactionView({
  error,
  status,
  summary,
}: {
  error?: string;
  status: CompactionStatus;
  summary?: string;
}) {
  const [open, setOpen] = useState(false);
  const label = STATUS_LABEL[status];
  const icon = status === "in_progress"
    ? <LoaderCircle aria-hidden="true" className="compaction-spinner" size={13} />
    : status === "failed" ? <CircleAlert aria-hidden="true" className="compaction-failed-icon" size={13} /> : null;
  const detail = status === "failed" && error ? <span className="compaction-error">{error}</span> : null;

  return (
    <section className="compaction-row" data-open={open} data-status={status}>
      {summary ? (
        <button
          aria-expanded={open}
          aria-label={`${open ? "Collapse" : "Expand"} context summary`}
          className="compaction-heading"
          onClick={() => setOpen((value) => !value)}
          type="button"
        >
          <ChevronDown aria-hidden="true" className="agent-plan-chevron" size={13} />
          {icon}
          <span>{label}</span>
          {detail}
        </button>
      ) : (
        <div className="compaction-heading" role="status">
          {icon}
          <span>{label}</span>
          {detail}
        </div>
      )}
      {summary ? (
        <div
          aria-hidden={!open}
          className="compaction-disclosure"
          data-open={open}
          inert={open ? undefined : true}
        >
          <div className="compaction-disclosure-content">
            <AgentMarkdown className="compaction-summary" text={summary} />
          </div>
        </div>
      ) : null}
    </section>
  );
}
