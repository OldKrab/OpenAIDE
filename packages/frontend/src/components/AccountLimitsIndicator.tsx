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
 * the same windows. The Composer corner is their only resting place; the corner arcs share the
 * Composer corner's centre of curvature, so they stay inside its padding at any draft length.
 */

const WINDOW_MS: Record<AgentAccountLimitWindow["kind"], number> = {
  fiveHour: 5 * 60 * 60 * 1000,
  weekly: 7 * 24 * 60 * 60 * 1000,
  weeklyModel: 7 * 24 * 60 * 60 * 1000,
};

const ANNOUNCE_MS = 2600;
const CLOCK_TICK_MS = 60_000;

export function AccountLimitsIndicator({
  agentLabel,
  compact,
  hostRef,
  limits,
  now = Date.now,
}: {
  /** Names whose account the limits belong to; the limits themselves carry no Agent identity. */
  agentLabel: string;
  compact: boolean;
  hostRef: RefObject<HTMLDivElement | null>;
  limits?: AgentAccountLimits | null;
  /** Injectable clock so reset labels and elapsed ticks are deterministic in tests. */
  now?: () => number;
}) {
  const [open, setOpen] = useState(false);
  const radius = useComposerCornerRadius(hostRef, compact);
  const clock = useMinuteClock(now, limits !== undefined);
  const primary = limits ? headlineWindow(limits.windows) : undefined;
  const announce = useStepAnnouncement(primary);

  if (!limits || !primary) return null;

  const rings = ringWindows(limits.windows, compact);
  // The compact Composer has no room for two lines beside the corner, so it names the headline only.
  const summary = (compact ? [primary] : limits.windows.filter((window) => window.kind !== "weeklyModel"))
    .map((window) => ({ window, text: `${windowLabel(window)} ${amountLabel(window)}`, reset: resetLabel(window, clock) }));

  return (
    <>
      <PopupPanel
        anchorRef={hostRef}
        className="account-limits-popup"
        label={`${agentLabel} limits`}
        onOpenChange={setOpen}
        open={open}
        placement="top-end"
        trigger={(props) => (
          <button
            {...props}
            aria-label={`${agentLabel} ${windowLabel(primary).toLowerCase()}: ${amountLabel(primary)}. Show limits`}
            className={`account-limits-corner account-limits-${primary.status}`}
            data-compact={compact}
            type="button"
          >
            <CornerRings clock={clock} radius={radius} windows={rings} />
          </button>
        )}
      >
        <AccountLimitsDetails agentLabel={agentLabel} clock={clock} limits={limits} onClose={() => setOpen(false)} />
      </PopupPanel>
      {!open ? (
        <span className="account-limits-label" data-announce={announce} role="tooltip">
          {summary.map(({ window, text, reset }) => (
            <span className={`account-limits-${window.status}`} key={window.kind}>
              {text}{reset ? ` · resets ${reset}` : ""}
            </span>
          ))}
        </span>
      ) : null}
    </>
  );
}

function CornerRings({ clock, radius, windows }: { clock: number; radius: number; windows: AgentAccountLimitWindow[] }) {
  const size = radius + 2;
  const centerX = 2;
  const centerY = radius;
  const arc = (r: number) => `M${centerX} ${centerY - r} A${r} ${r} 0 0 1 ${centerX + r} ${centerY}`;
  return (
    <svg aria-hidden="true" className="account-limits-rings" height={size} viewBox={`0 0 ${size} ${size}`} width={size}>
      {windows.map((window, index) => {
        const r = radius - 3.5 - index * 4;
        const elapsed = elapsedFraction(window, clock);
        return (
          <g className={`account-limits-ring account-limits-${window.status}`} data-inner={index > 0} key={window.kind}>
            <path className="account-limits-ring-track" d={arc(r)} />
            <path
              className="account-limits-ring-fill"
              d={arc(r)}
              pathLength="100"
              style={{ "--account-limit-used": clampPercent(window.usedPercent) } as CSSProperties}
            />
            {elapsed !== undefined && window.status !== "reached" ? (
              // Where the clock is in the window: a fill ahead of its tick is burning faster than time.
              <path className="account-limits-ring-tick" d={tickPath(centerX, centerY, r, elapsed)} />
            ) : null}
          </g>
        );
      })}
    </svg>
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
        <strong>{agentLabel} limits</strong>
        <button aria-label="Close limits" onClick={onClose} type="button">
          <X aria-hidden="true" size={15} />
        </button>
      </div>
      <p className="account-limits-scope">
        {limits.planLabel ? <strong>{limits.planLabel} plan</strong> : null}
        <span>Shared by all tasks</span>
      </p>
      {limits.windows.map((window) => {
        const reset = resetLabel(window, clock);
        const remaining = remainingLabel(window, clock);
        return (
          <div className={`account-limits-meter account-limits-${window.status}`} key={`${window.kind}:${window.modelLabel ?? ""}`}>
            <div className="account-limits-meter-heading">
              <span>{windowLabel(window)}</span>
              <b>{window.status === "reached" ? "Reached" : `${clampPercent(window.usedPercent)}% used`}</b>
            </div>
            <div aria-hidden="true" className="account-limits-meter-track">
              <span style={{ width: `${clampPercent(window.usedPercent)}%` }} />
            </div>
            {reset ? <small>Resets {reset}{remaining ? ` · ${remaining}` : ""}</small> : null}
          </div>
        );
      })}
    </section>
  );
}

/** Matches the edge meter: the Composer's computed corner is the only source of the arc geometry. */
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

/**
 * Outer arc is the 5-hour window, inner arc the week. The compact corner is too tight for a
 * readable inner arc, so it draws the headline window alone.
 */
function ringWindows(windows: AgentAccountLimitWindow[], compact: boolean) {
  const shared = windows.filter((window) => window.kind !== "weeklyModel");
  if (!compact) return shared.slice(0, 2);
  const headline = headlineWindow(shared);
  return headline ? [headline] : [];
}

function tickPath(centerX: number, centerY: number, r: number, elapsed: number) {
  const angle = elapsed * (Math.PI / 2);
  const point = (distance: number) =>
    `${round(centerX + distance * Math.sin(angle))} ${round(centerY - distance * Math.cos(angle))}`;
  return `M${point(r - 1.6)} L${point(r + 1.6)}`;
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
  return window.status === "reached" ? "reached" : `${clampPercent(window.usedPercent)}%`;
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

function remainingLabel(window: AgentAccountLimitWindow, clock: number) {
  const resetsAt = parseInstant(window.resetsAtMs);
  if (resetsAt === undefined) return undefined;
  const minutes = Math.round((resetsAt - clock) / 60_000);
  if (minutes <= 0) return undefined;
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return minutes % 60 ? `in ${hours} h ${minutes % 60} min` : `in ${hours} h`;
  const days = Math.round(hours / 24);
  return `in ${days} ${days === 1 ? "day" : "days"}`;
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
