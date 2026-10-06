import { useState } from "react";
import { ChevronRight, ChevronUp, CircleAlert } from "lucide-react";
import type { CompactionStatus, TimeSpan } from "@openaide/app-shell-contracts";
import { AgentMarkdown } from "./AgentMarkdown";
import {
  ELAPSED_VISIBLE_AFTER_SECONDS,
  elapsedDurationLabel,
  formatElapsedDuration,
  useElapsedSeconds,
} from "./elapsedDuration";
import { CompactionDuration } from "./timeMarks";

const STATUS_LABEL: Record<CompactionStatus, string> = {
  in_progress: "Compacting context…",
  completed: "Context compacted",
  failed: "Context compaction failed",
  cancelled: "Context compaction cancelled",
  unknown: "Context compaction",
};

/**
 * Chat row for an Agent-owned context compaction: a boundary rule with its label on the line.
 *
 * While the turn runs, the in-progress row is the Task's live indicator and replaces the working
 * footer, so it keeps one position from start to finish. The Agent's summary is model-facing: it
 * stays collapsed, and opening it turns the rule into a frame so the text reads as part of this row.
 */
export function CompactionView({
  error,
  live = false,
  liveStartedAt,
  run,
  status,
  summary,
}: {
  error?: string;
  /** The Task's turn is running, so an in-progress row animates and carries the elapsed time. */
  live?: boolean;
  liveStartedAt?: string;
  /** How long the compaction itself took, when the App Server watched it run. */
  run?: TimeSpan;
  status: CompactionStatus;
  summary?: string;
}) {
  const [open, setOpen] = useState(false);
  const failed = status === "failed";
  const running = live && status === "in_progress";
  const toggle = () => setOpen((value) => !value);
  const label = running ? (
    <>
      <CompactionGlyph />
      <span className="compaction-label-text">Compacting context</span>
      <CompactionElapsed startedAt={liveStartedAt} />
    </>
  ) : (
    <>
      {failed ? <CircleAlert aria-hidden="true" className="compaction-failed-icon" size={13} /> : <CompactionGlyph />}
      <span className="compaction-label-text">{STATUS_LABEL[status]}</span>
      {status === "in_progress" ? null : <CompactionDuration run={run} />}
      {summary ? <ChevronRight aria-hidden="true" className="compaction-chevron" size={12} /> : null}
    </>
  );

  return (
    <section className="compaction-row" data-live={running} data-open={open} data-status={status}>
      <div className="compaction-head">
        <span aria-hidden="true" className="compaction-rule" />
        {summary && !running ? (
          <button
            aria-expanded={open}
            aria-label={`${open ? "Collapse" : "Expand"} context summary`}
            className="compaction-label"
            onClick={toggle}
            type="button"
          >
            {label}
          </button>
        ) : (
          <div aria-live={running ? "polite" : undefined} className="compaction-label" role="status">
            {label}
          </div>
        )}
        <span aria-hidden="true" className="compaction-rule" />
      </div>
      {failed && error ? <p className="compaction-error">{error}</p> : null}
      {summary && !running ? (
        <div
          aria-hidden={!open}
          className="compaction-disclosure"
          data-open={open}
          inert={open ? undefined : true}
        >
          <div className="compaction-disclosure-content">
            <AgentMarkdown className="compaction-summary" text={summary} />
            <button className="compaction-collapse" onClick={toggle} type="button">
              <ChevronUp aria-hidden="true" size={12} />
              <span>Hide summary</span>
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function CompactionElapsed({ startedAt }: { startedAt?: string }) {
  const elapsedSeconds = useElapsedSeconds(startedAt);
  if (elapsedSeconds === undefined || elapsedSeconds < ELAPSED_VISIBLE_AFTER_SECONDS) return null;
  return (
    <>
      <span aria-hidden="true" className="working-status-duration-separator" />
      <time
        aria-label={`Elapsed time ${elapsedDurationLabel(elapsedSeconds)}`}
        className="compaction-elapsed"
        dateTime={`PT${elapsedSeconds}S`}
      >
        {formatElapsedDuration(elapsedSeconds)}
      </time>
    </>
  );
}

/** Two arrows closing on a line. No Tool uses it, so compaction never reads as "the latest command". */
function CompactionGlyph() {
  return (
    <svg
      aria-hidden="true"
      className="compaction-glyph"
      fill="none"
      height="14"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.8"
      viewBox="0 0 24 24"
      width="14"
    >
      <g className="compaction-glyph-top"><path d="M12 2v6" /><path d="m15 5-3 3-3-3" /></g>
      <path d="M4 12h16" />
      <g className="compaction-glyph-bottom"><path d="M12 22v-6" /><path d="m15 19-3-3-3 3" /></g>
    </svg>
  );
}
