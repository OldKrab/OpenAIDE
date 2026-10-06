import { Timer } from "lucide-react";
import { useLayoutEffect, useRef, type Ref } from "react";
import type { ActivityStep, TimeSpan } from "@openaide/app-shell-contracts";
import { elapsedDurationLabel, formatElapsedDuration, useElapsedSeconds } from "./elapsedDuration";
import { timestampMillis } from "./taskSurfaceHelpers";

/**
 * Time marks for Chat and Navigation.
 *
 * Two readings never share a shape: a duration ("how long") carries the timer glyph or sits in a
 * labelled slot, and a bare `18:40` is always a clock time ("when"). Every mark renders nothing
 * when the App Server did not observe the time, so reloaded history shows no guessed value.
 */

/** A finished duration: `42s`, `4m 12s`, `1h 04m`. Live clocks use `formatElapsedDuration`. */
export function formatSettledDuration(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  if (minutes > 0) return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  return `${seconds}s`;
}

/** Whole seconds of a finished span; undefined while it runs or when a stamp is unreadable. */
export function settledSpanSeconds(span: TimeSpan | undefined) {
  if (!span?.ended_at) return undefined;
  const startedAt = timestampMillis(span.started_at);
  const endedAt = timestampMillis(span.ended_at);
  if (Number.isNaN(startedAt) || Number.isNaN(endedAt) || endedAt < startedAt) return undefined;
  return Math.round((endedAt - startedAt) / 1_000);
}

const clockFormat = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });
const dateTimeFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

function parsedDate(value: string | undefined) {
  const millis = value ? timestampMillis(value) : Number.NaN;
  return Number.isNaN(millis) ? undefined : new Date(millis);
}

/** The local clock time of an App Server stamp, or undefined when it is unreadable. */
export function formatClockTime(value: string | undefined) {
  const date = parsedDate(value);
  return date ? clockFormat.format(date) : undefined;
}

function SettledDuration({ className, seconds }: { className?: string; seconds: number }) {
  return (
    <time className={className} dateTime={`PT${seconds}S`}>
      {formatSettledDuration(seconds)}
    </time>
  );
}

/** A ticking `m:ss`; the interval lives here so the surrounding row does not re-render each second. */
export function LiveDuration({ className, startedAt }: { className?: string; startedAt: string }) {
  const seconds = useElapsedSeconds(startedAt);
  if (seconds === undefined) return null;
  return (
    <time
      aria-label={`Elapsed time ${elapsedDurationLabel(seconds)}`}
      className={className}
      dateTime={`PT${seconds}S`}
    >
      {formatElapsedDuration(seconds)}
    </time>
  );
}

/** When a user message was sent. Clock only: a duration here would read as the Agent's work time. */
export function SentClock({ at }: { at?: string }) {
  const date = parsedDate(at);
  if (!date) return null;
  return (
    <time className="chat-message-clock" dateTime={date.toISOString()} title={`Sent ${dateTimeFormat.format(date)}`}>
      {clockFormat.format(date)}
    </time>
  );
}

/** How long the turn that this answer closed took, shown beside the answer's actions. */
export function TurnDuration({ turn }: { turn?: TimeSpan }) {
  const seconds = settledSpanSeconds(turn);
  if (seconds === undefined) return null;
  const finishedAt = formatClockTime(turn?.ended_at);
  const duration = formatSettledDuration(seconds);
  return (
    <span
      className="chat-action-time"
      title={finishedAt ? `Worked for ${duration}, finished ${finishedAt}` : `Worked for ${duration}`}
    >
      <Timer aria-hidden="true" size={12} strokeWidth={1.75} />
      <span className="visually-hidden">Worked for </span>
      <SettledDuration seconds={seconds} />
    </span>
  );
}

/** Trails the line that reports a stopped turn; always visible because the line is the turn's end. */
export function StoppedTurnDuration({ turn }: { turn?: TimeSpan }) {
  const seconds = settledSpanSeconds(turn);
  if (seconds === undefined) return null;
  return (
    <span className="chat-stop-duration" title={`Stopped after ${formatSettledDuration(seconds)}`}>
      {" · "}
      <SettledDuration seconds={seconds} />
    </span>
  );
}

/** A finished compaction's duration, set off from its label like the live elapsed clock. */
export function CompactionDuration({ run }: { run?: TimeSpan }) {
  const seconds = settledSpanSeconds(run);
  if (seconds === undefined) return null;
  return (
    <>
      <span aria-hidden="true" className="working-status-duration-separator" />
      <SettledDuration className="compaction-elapsed" seconds={seconds} />
    </>
  );
}

/** How long a pending Permission or Question has been waiting for the user. */
export function RequestWait({
  className = "request-wait",
  label = "waiting",
  since,
}: {
  className?: string;
  label?: string;
  since?: string;
}) {
  if (!since || Number.isNaN(timestampMillis(since))) return null;
  return (
    <span className={className}>
      {label} <LiveDuration startedAt={since} />
    </span>
  );
}

function TimedMark({
  className,
  live,
  markRef,
  span,
}: {
  className: string;
  live: boolean;
  markRef?: Ref<HTMLSpanElement>;
  span: TimeSpan;
}) {
  const seconds = settledSpanSeconds(span);
  if (!live && seconds === undefined) return null;
  return (
    <span className={`time-mark ${className}`} data-live={live ? "true" : undefined} ref={markRef}>
      <Timer aria-hidden="true" size={12} strokeWidth={1.75} />
      {live ? <LiveDuration startedAt={span.started_at} /> : <SettledDuration seconds={seconds ?? 0} />}
    </span>
  );
}

function stepRun(step: ActivityStep) {
  return step.kind === "tool" || step.kind === "command" || step.kind === "subagent"
    ? { run: step.run, running: step.status === "running" }
    : undefined;
}

/** One step's duration: revealed on hover once finished, always visible and ticking while it runs. */
export function ActivityStepTime({ step }: { step: ActivityStep }) {
  const timed = stepRun(step);
  if (!timed?.run) return null;
  // A span left open by a step that is no longer running has no trustworthy end.
  const live = timed.running && !timed.run.ended_at;
  return <TimedMark className="activity-step-time" live={live} span={timed.run} />;
}

/** The wall-clock span covered by a group's timed steps: first start to last end. */
export function activityGroupSpan(steps: ActivityStep[], fallback?: TimeSpan): { live: boolean; span: TimeSpan } | undefined {
  let startedAt: string | undefined;
  let endedAt: string | undefined;
  let live = false;
  for (const step of steps) {
    const timed = stepRun(step);
    if (!timed?.run) continue;
    if (startedAt === undefined || timestampMillis(timed.run.started_at) < timestampMillis(startedAt)) {
      startedAt = timed.run.started_at;
    }
    if (timed.run.ended_at) {
      if (endedAt === undefined || timestampMillis(timed.run.ended_at) > timestampMillis(endedAt)) {
        endedAt = timed.run.ended_at;
      }
    } else if (timed.running) {
      live = true;
    }
  }
  if (startedAt === undefined) return fallback ? { live: false, span: fallback } : undefined;
  if (live) return { live: true, span: { started_at: startedAt } };
  return endedAt ? { live: false, span: { started_at: startedAt, ended_at: endedAt } } : undefined;
}

/** Room the trailing time needs past the end of a group header before it must overlay instead. */
const GROUP_TIME_TRAILING_ROOM_PX = 90;

/**
 * A group's total, revealed on hover of its header. It takes no room in the header, so a long
 * summary keeps its full measure: the time trails the header, or overlays its end when the header
 * already fills the row.
 */
export function ActivityGroupTime({ fallback, steps }: { fallback?: TimeSpan; steps: ActivityStep[] }) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const total = activityGroupSpan(steps, fallback);
  const shown = Boolean(total);
  // A group whose total appears or changes kind re-measures; a ticking second does not.
  const live = total?.live;
  useLayoutEffect(() => {
    const mark = ref.current;
    const header = mark?.parentElement;
    const row = header?.parentElement;
    if (!mark || !header || !row) return undefined;
    const measure = () => {
      mark.dataset.overlay = String(row.clientWidth - header.offsetWidth < GROUP_TIME_TRAILING_ROOM_PX);
    };
    measure();
    // Re-measured when the reader reaches for the header, so a resized Chat needs no observer.
    header.addEventListener("pointerenter", measure);
    header.addEventListener("focus", measure);
    return () => {
      header.removeEventListener("pointerenter", measure);
      header.removeEventListener("focus", measure);
    };
  }, [shown, live]);
  if (!total) return null;
  return <TimedMark className="activity-group-time" live={total.live} markRef={ref} span={total.span} />;
}
