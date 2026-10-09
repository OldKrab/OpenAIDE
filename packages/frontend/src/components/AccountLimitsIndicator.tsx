import { X } from "lucide-react";
import {
  type CSSProperties,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { AgentAccountLimits, AgentAccountLimitWindow } from "@openaide/app-server-client";
import { PopupPanel } from "./Popup";

/*
 * Account limits belong to the Agent's subscription, not to a Task: every Task of that Agent shows
 * the same windows. They rest on the right end of the Composer's top border, the same way the
 * context meter rests on its right border, so neither takes space from the draft.
 */

const WINDOW_MS: Record<AgentAccountLimitWindow["kind"], number> = {
  fiveHour: 5 * 60 * 60 * 1000,
  weekly: 7 * 24 * 60 * 60 * 1000,
  weeklyModel: 7 * 24 * 60 * 60 * 1000,
};

const ANNOUNCE_MS = 2600;
/** Keeps the meters on the straight part of the border, clear of the corner curve. */
const EDGE_CORNER_GAP = 4;
const EDGE_HEIGHT = 8;
/** Points of usage a window may run ahead of its elapsed time before it reads as draining fast. */
const FAST_MARGIN_PERCENT = 5;
const CLOCK_TICK_MS = 60_000;

export function AccountLimitsIndicator({
  agentLabel,
  compact,
  hostRef,
  limits,
  now = Date.now,
  onOpenChange,
  open,
}: {
  /** Names whose account the limits belong to; the limits themselves carry no Agent identity. */
  agentLabel: string;
  compact: boolean;
  hostRef: RefObject<HTMLDivElement | null>;
  limits?: AgentAccountLimits | null;
  /** Injectable clock so reset labels and elapsed ticks are deterministic in tests. */
  now?: () => number;
  /** The Composer owns which of its panels is open, so this one never stacks on another. */
  onOpenChange: (open: boolean) => void;
  open: boolean;
}) {
  const radius = useComposerCornerRadius(hostRef, compact);
  const clock = useMinuteClock(now, limits !== undefined);
  const primary = limits ? headlineWindow(limits.windows) : undefined;
  const announce = useStepAnnouncement(primary);

  if (!limits || !primary) return null;

  const segments = edgeWindows(limits.windows);
  // The compact Composer has no room for two lines beside the corner, so it names the headline only.
  const summary = (compact ? [primary] : limits.windows.filter((window) => window.kind !== "weeklyModel"))
    .map((window) => ({ window, text: `${windowLabel(window)} ${amountLabel(window)}`, reset: resetLabel(window, clock) }));

  return (
    <>
      <PopupPanel
        anchorRef={hostRef}
        className="account-limits-popup"
        label={`${agentLabel} limits`}
        onOpenChange={onOpenChange}
        open={open}
        placement="top-end"
        trigger={(props) => (
          <button
            {...props}
            aria-label={`${agentLabel} ${windowLabel(primary).toLowerCase()}: ${amountLabel(primary)}. Show limits`}
            className={`account-limits-edge account-limits-${windowTone(primary, clock)}`}
            data-compact={compact}
            style={{ right: radius + EDGE_CORNER_GAP }}
            type="button"
          >
            <EdgeSegments clock={clock} length={compact ? 36 : 52} windows={segments} />
          </button>
        )}
      >
        <AccountLimitsDetails agentLabel={agentLabel} clock={clock} limits={limits} onClose={() => onOpenChange(false)} />
      </PopupPanel>
      {!open ? (
        <span className="account-limits-label" data-announce={announce} role="tooltip">
          {summary.map(({ window, text, reset }) => (
            <span className={`account-limits-${windowTone(window, clock)}`} key={window.kind}>
              {text}{reset ? ` · resets ${reset}` : ""}
            </span>
          ))}
        </span>
      ) : null}
    </>
  );
}

/**
 * One straight meter per window, laid along the top border. It starts full and drains toward its
 * name as the limit is spent. Each carries its window's name on the border, the way a fieldset
 * legend does, because two bare lines cannot say which limit is which.
 */
function EdgeSegments({ clock, length, windows }: { clock: number; length: number; windows: AgentAccountLimitWindow[] }) {
  const line = `M0 ${EDGE_HEIGHT / 2} h${length}`;
  return (
    <>
      {windows.map((window) => {
        const elapsed = elapsedFraction(window, clock);
        return (
          <span aria-hidden="true" className={`account-limits-segment account-limits-${windowTone(window, clock)}`} key={window.kind}>
            <span className="account-limits-segment-name">{window.kind === "fiveHour" ? "5h" : "week"}</span>
            <svg height={EDGE_HEIGHT} viewBox={`0 0 ${length} ${EDGE_HEIGHT}`} width={length}>
              <path className="account-limits-segment-track" d={line} />
              <path
                className="account-limits-segment-fill"
                d={line}
                pathLength="100"
                style={{ "--account-limit-left": leftPercent(window) } as CSSProperties}
              />
              {elapsed !== undefined && window.status !== "reached" ? (
                // How much of the window's time is left: a fill that falls short of its tick is draining faster than time.
                <path className="account-limits-segment-tick" d={`M${round((1 - elapsed) * length)} 1 v${EDGE_HEIGHT - 2}`} />
              ) : null}
            </svg>
          </span>
        );
      })}
    </>
  );
}

function AccountLimitsDetails({
  agentLabel,
  clock,
  limits,
  onClose,
}: {
  agentLabel: string;
  clock: number;
  limits: AgentAccountLimits;
  onClose: () => void;
}) {
  return (
    <section className="account-limits-panel">
      <div className="context-usage-panel-header">
        <strong>
          {agentLabel} limits
          {limits.planLabel ? <span className="account-limits-plan">{limits.planLabel}</span> : null}
        </strong>
        <button aria-label="Close limits" onClick={onClose} type="button">
          <X aria-hidden="true" size={15} />
        </button>
      </div>
      {limits.windows.map((window) => (
        // One line of words per window: its name, when it resets, and how much is used.
        <div className={`account-limits-meter account-limits-${windowTone(window, clock)}`} key={`${window.kind}:${window.modelLabel ?? ""}`}>
          <div className="account-limits-meter-heading">
            <span>{panelWindowLabel(window)}</span>
            <small title={resetLabel(window, clock)}>{panelResetLabel(window, clock)}</small>
            <b>{window.status === "reached" ? "Used up" : `${leftPercent(window)}% left`}</b>
          </div>
          <div aria-hidden="true" className="account-limits-meter-track">
            <span style={{ width: `${leftPercent(window)}%` }} />
            <MeterTick clock={clock} window={window} />
          </div>
        </div>
      ))}
    </section>
  );
}

/** Same mark as on the Composer border: the share of the window's time still ahead. */
function MeterTick({ clock, window }: { clock: number; window: AgentAccountLimitWindow }) {
  const elapsed = elapsedFraction(window, clock);
  if (elapsed === undefined) return null;
  return <i className="account-limits-meter-tick" style={{ left: `${round((1 - elapsed) * 100)}%` }} />;
}

/** The Composer's computed corner decides where the straight part of its top border ends. */
function useComposerCornerRadius(hostRef: RefObject<HTMLDivElement | null>, compact: boolean) {
  const [radius, setRadius] = useState(compact ? 14 : 20);
  useLayoutEffect(() => {
    const composer = hostRef.current?.querySelector<HTMLElement>(".composer");
    if (!composer) return;
    const computed = Number.parseFloat(getComputedStyle(composer).borderTopRightRadius);
    setRadius(Number.isFinite(computed) && computed >= 10 ? computed : compact ? 14 : 20);
  }, [compact, hostRef]);
  return radius;
}

/** Reset labels and elapsed ticks only need minute precision, so one quiet timer serves both. */
function useMinuteClock(now: () => number, active: boolean) {
  const [clock, setClock] = useState(now);
  useEffect(() => {
    if (!active) return undefined;
    setClock(now());
    const timer = window.setInterval(() => setClock(now()), CLOCK_TICK_MS);
    return () => window.clearInterval(timer);
  }, [active, now]);
  return clock;
}

/**
 * The fill moves on every update, but the label only speaks up when usage crosses a 10% step, so
 * most turns leave the corner still.
 */
function useStepAnnouncement(primary: AgentAccountLimitWindow | undefined) {
  const [announce, setAnnounce] = useState(false);
  const step = primary ? Math.floor(clampPercent(primary.usedPercent) / 10) : undefined;
  const key = primary?.kind;
  const seen = useRef<{ key?: string; step?: number }>({ key, step });
  useEffect(() => {
    const previous = seen.current;
    seen.current = { key, step };
    if (step === undefined || previous.step === undefined || previous.key !== key || step <= previous.step) {
      return undefined;
    }
    setAnnounce(true);
    const timer = window.setTimeout(() => setAnnounce(false), ANNOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [key, step]);
  return announce;
}

/** The window the user must hear about first: reached, then warning, else the 5-hour window. */
export function headlineWindow(windows: AgentAccountLimitWindow[]) {
  return windows.find((window) => window.status === "reached")
    ?? windows.find((window) => window.status === "warning")
    ?? windows.find((window) => window.kind === "fiveHour")
    ?? windows[0];
}

/** Left to right in reading order: the 5-hour window, then the week. Per-model windows stay in the panel. */
function edgeWindows(windows: AgentAccountLimitWindow[]) {
  return windows.filter((window) => window.kind !== "weeklyModel").slice(0, 2);
}

function elapsedFraction(window: AgentAccountLimitWindow, clock: number) {
  const resetsAt = parseInstant(window.resetsAtMs);
  if (resetsAt === undefined) return undefined;
  const remaining = resetsAt - clock;
  if (remaining <= 0 || remaining > WINDOW_MS[window.kind]) return undefined;
  return 1 - remaining / WINDOW_MS[window.kind];
}

function windowLabel(window: AgentAccountLimitWindow) {
  if (window.kind === "fiveHour") return "5-hour limit";
  if (window.kind === "weekly") return "Weekly limit";
  return window.modelLabel ? `Weekly · ${window.modelLabel}` : "Weekly model limit";
}

function amountLabel(window: AgentAccountLimitWindow) {
  return window.status === "reached" ? "used up" : `${leftPercent(window)}% left`;
}

function leftPercent(window: AgentAccountLimitWindow) {
  return 100 - clampPercent(window.usedPercent);
}

/**
 * The colour a window is drawn in. The Agent's own warning and reached states come first; below
 * them, `fast` marks a window draining faster than its time runs out. It is a pace hint drawn from
 * the reading, never a status, and the margin keeps the first turns of a fresh window quiet.
 */
function windowTone(window: AgentAccountLimitWindow, clock: number) {
  if (window.status !== "ok") return window.status;
  const elapsed = elapsedFraction(window, clock);
  if (elapsed === undefined) return "ok";
  return clampPercent(window.usedPercent) - elapsed * 100 >= FAST_MARGIN_PERCENT ? "fast" : "ok";
}

/** Clock time for a reset later today, otherwise the day and time, in the viewer's locale. */
export function resetLabel(window: AgentAccountLimitWindow, clock: number) {
  const resetsAt = parseInstant(window.resetsAtMs);
  if (resetsAt === undefined) return undefined;
  const reset = new Date(resetsAt);
  const time = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(reset);
  if (reset.toDateString() === new Date(clock).toDateString()) return `at ${time}`;
  const day = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short" }).format(reset);
  return `on ${day}, ${time}`;
}

function panelWindowLabel(window: AgentAccountLimitWindow) {
  if (window.kind === "fiveHour") return "5-hour";
  if (window.kind === "weekly") return "Week";
  return window.modelLabel ? `Week · ${window.modelLabel}` : "Week · model";
}

/** A reset within a day reads as a countdown; a later one as the day it lands on. */
function panelResetLabel(window: AgentAccountLimitWindow, clock: number) {
  const resetsAt = parseInstant(window.resetsAtMs);
  if (resetsAt === undefined) return undefined;
  const minutes = Math.round((resetsAt - clock) / 60_000);
  if (minutes <= 0) return undefined;
  if (minutes < 60) return `resets in ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return minutes % 60 ? `resets in ${hours} h ${minutes % 60} min` : `resets in ${hours} h`;
  const day = new Intl.DateTimeFormat(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" }).format(new Date(resetsAt));
  return `resets ${day}`;
}

function parseInstant(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function clampPercent(value: number) {
  return Math.round(Math.min(100, Math.max(0, value)));
}

function round(value: number) {
  return Math.round(value * 100) / 100;
}
