import { X } from "lucide-react";
import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import type {
  ConfigOptionsCatalog,
  TaskContextUsage,
} from "@openaide/app-shell-contracts";

type UsageTone = "normal" | "high" | "critical";
type PointerPosition = { x: number; y: number };

export function ComposerWithContextUsage({
  children,
  configOptions,
  usage,
}: {
  children: ReactNode;
  configOptions?: ConfigOptionsCatalog;
  usage?: TaskContextUsage;
}) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [pointerPosition, setPointerPosition] = useState<PointerPosition>();
  const hostRef = useRef<HTMLDivElement>(null);
  const detailsId = useId().replaceAll(":", "");
  const capacity = usage?.capacity_tokens ?? 0;
  const usagePercent = capacity > 0
    ? Math.round(Math.min(100, Math.max(0, ((usage?.used_tokens ?? 0) / capacity) * 100)))
    : 0;
  const tone: UsageTone = usagePercent >= 95
    ? "critical"
    : usagePercent >= 75 ? "high" : "normal";

  const closeDetails = () => {
    setDetailsOpen(false);
  };
  const rememberPointer = (event: ReactPointerEvent) => {
    if (event.pointerType !== "mouse") return;
    setPointerPosition({ x: event.clientX, y: event.clientY });
  };

  useEffect(() => {
    if (!detailsOpen) return undefined;
    const closeOnOutsideInteraction = (event: PointerEvent) => {
      if (!hostRef.current?.contains(event.target as Node)) closeDetails();
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDetails();
    };
    document.addEventListener("pointerdown", closeOnOutsideInteraction);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsideInteraction);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [detailsOpen]);

  useEffect(() => {
    if (!usage) closeDetails();
  }, [usage]);

  return (
    <div className="composer-context-host" ref={hostRef}>
      {children}
      {usage && capacity > 0 ? (
        <div className="context-usage-interaction">
          <button
            aria-controls={detailsId}
            aria-expanded={detailsOpen}
            aria-haspopup="dialog"
            aria-label={`Context usage: ${usagePercent}% used. Show details`}
            className={`context-usage-meter context-usage-meter-${tone}`}
            onClick={() => setDetailsOpen((open) => !open)}
            onPointerEnter={(event) => {
              rememberPointer(event);
            }}
            onPointerLeave={() => setPointerPosition(undefined)}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="context-usage-edge"
              preserveAspectRatio="none"
              viewBox="0 0 20 100"
            >
              <path
                className="context-usage-edge-track"
                d="M 0 100 C 11 100 20 91 20 80 L 20 20 C 20 9 11 0 0 0 L 0 .8 C 10.6 .8 17.8 9.8 17.8 20 L 17.8 80 C 17.8 90.2 10.6 99.2 0 99.2 Z"
              />
              <path
                className="context-usage-edge-fill"
                d="M 0 100 C 11 100 20 91 20 80 L 20 20 C 20 9 11 0 0 0 L 0 .8 C 10.6 .8 17.8 9.8 17.8 20 L 17.8 80 C 17.8 90.2 10.6 99.2 0 99.2 Z"
                style={{ "--context-usage-percent": `${usagePercent}%` } as CSSProperties}
              />
            </svg>
            {!detailsOpen ? (
              <span
                className={`context-usage-tooltip${
                  pointerPosition ? " context-usage-tooltip-cursor" : ""
                }`}
                role="tooltip"
                style={pointerPosition
                  ? {
                    "--context-usage-cursor-x": `${pointerPosition.x}px`,
                    "--context-usage-cursor-y": `${pointerPosition.y}px`,
                  } as CSSProperties
                  : undefined}
              >
                Context used: {usagePercent}%
              </span>
            ) : null}
          </button>
          {detailsOpen ? (
            <ContextUsageDetails
              detailsId={detailsId}
              modelLabel={selectedModelLabel(configOptions)}
              onClose={closeDetails}
              tone={tone}
              usage={usage}
              usagePercent={usagePercent}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function ContextUsageDetails({
  detailsId,
  modelLabel,
  onClose,
  tone,
  usage,
  usagePercent,
}: {
  detailsId: string;
  modelLabel?: string;
  onClose: () => void;
  tone: UsageTone;
  usage: TaskContextUsage;
  usagePercent: number;
}) {
  const remaining = Math.max(0, usage.capacity_tokens - usage.used_tokens);
  const turn = usage.last_turn;
  return (
    <section
      aria-label="Context usage details"
      className={`context-usage-panel context-usage-panel-${tone} context-usage-panel-anchor`}
      data-placement="anchor"
      id={detailsId}
      role="dialog"
    >
      <div className="context-usage-panel-header">
        <strong>Context</strong>
        <button aria-label="Close context details" onClick={onClose} type="button">
          <X aria-hidden="true" size={15} />
        </button>
      </div>
      <div className="context-usage-current">
        <div className="context-usage-current-heading">
          <span>{usagePercent}% used</span>
          <span>{formatTokens(remaining)} available</span>
        </div>
        <div aria-hidden="true" className="context-usage-horizontal-track">
          <span style={{ width: `${usagePercent}%` }} />
        </div>
        <div className="context-usage-token-line">
          {formatTokens(usage.used_tokens)} of {formatTokens(usage.capacity_tokens)} tokens
        </div>
      </div>
      {turn ? (
        <div className="context-usage-last-turn">
          <div className="context-usage-section-heading">
            <strong>Last turn</strong>
            {modelLabel ? <span>{modelLabel}</span> : null}
          </div>
          <dl className="context-usage-breakdown">
            <UsageRow label="Processed" value={turn.total_tokens} />
            <UsageRow label="New input" value={turn.input_tokens} />
            {turn.cached_read_tokens !== undefined ? (
              <UsageRow label="Cache read" value={turn.cached_read_tokens} />
            ) : null}
            {turn.cached_write_tokens !== undefined ? (
              <UsageRow label="Cache write" value={turn.cached_write_tokens} />
            ) : null}
            <UsageRow
              detail={turn.reasoning_tokens !== undefined
                ? `${formatTokens(turn.reasoning_tokens)} reasoning`
                : undefined}
              label="Output"
              value={turn.output_tokens}
            />
          </dl>
        </div>
      ) : null}
      {usage.cost ? (
        <div className="context-usage-cost">
          <span>Session cost</span>
          <strong>{formatCost(usage.cost.amount, usage.cost.currency)}</strong>
        </div>
      ) : null}
    </section>
  );
}

function UsageRow({ detail, label, value }: { detail?: string; label: string; value: number }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>
        {formatTokens(value)}
        {detail ? <small>{detail}</small> : null}
      </dd>
    </div>
  );
}

function formatTokens(tokens: number) {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
  return tokens.toLocaleString();
}

function formatCost(amount: string, currency: string) {
  const numericAmount = Number(amount);
  if (!Number.isFinite(numericAmount)) return `${amount} ${currency}`;
  try {
    return new Intl.NumberFormat(undefined, {
      style: "currency",
      currency,
      maximumFractionDigits: 4,
    }).format(numericAmount);
  } catch {
    return `${amount} ${currency}`;
  }
}

function selectedModelLabel(configOptions?: ConfigOptionsCatalog) {
  const model = configOptions?.options.find((option) => option.category === "model");
  if (!model || model.current_value.type !== "id") return undefined;
  return model.values.find((value) => value.id === model.current_value.value)?.label
    ?? model.current_value.value;
}
