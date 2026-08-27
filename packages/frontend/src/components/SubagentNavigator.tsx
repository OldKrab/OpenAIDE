import { Bot, ChevronDown, CornerUpLeft } from "lucide-react";
import type { SubagentCatalogEntrySnapshot } from "@openaide/app-server-client";

export function SubagentNavigator({
  entries,
  onSelect,
  selectedId,
  unseen,
}: {
  entries: SubagentCatalogEntrySnapshot[];
  onSelect: (subagentId?: string) => void;
  selectedId?: string;
  unseen: Set<string>;
}) {
  if (entries.length === 0) return null;
  const selected = entries.find((entry) => entry.subagentId === selectedId);
  const depth = hierarchyDepths(entries);
  return (
    <nav aria-label="Agent histories" className="subagent-navigator">
      <button
        aria-keyshortcuts="Alt+ArrowLeft"
        className="subagent-main-button"
        data-selected={!selected}
        onClick={() => onSelect(undefined)}
        title={selected ? "Return to Main Agent (Alt+Left)" : "Main Agent"}
        type="button"
      >
        {selected ? <CornerUpLeft aria-hidden="true" size={14} /> : <Bot aria-hidden="true" size={14} />}
        <span>Main Agent</span>
      </button>
      <label className="subagent-history-select">
        <span className="sr-only">Switch agent history</span>
        <select
          aria-label="Switch agent history"
          onChange={(event) => onSelect(event.target.value || undefined)}
          value={selectedId ?? ""}
        >
          <option value="">{selected ? "Switch subagent" : `${entries.length} subagent${entries.length === 1 ? "" : "s"}`}</option>
          {entries.map((entry) => (
            <option key={entry.subagentId} value={entry.subagentId}>
              {`${"  ".repeat(depth.get(entry.subagentId) ?? 0)}${statusGlyph(entry.status)} ${entry.name}${unseen.has(entry.subagentId) ? " • New" : ""}`}
            </option>
          ))}
        </select>
        <ChevronDown aria-hidden="true" size={14} />
      </label>
      {selected ? (
        <div className="subagent-current" role="status">
          <span className={`subagent-status-dot ${selected.status}`} aria-hidden="true" />
          <span>{selected.name}</span>
          <small>{statusLabel(selected.status)}</small>
        </div>
      ) : null}
    </nav>
  );
}

function hierarchyDepths(entries: SubagentCatalogEntrySnapshot[]) {
  const parents = new Map(entries.map((entry) => [entry.subagentId, entry.parentSubagentId]));
  return new Map(entries.map((entry) => {
    let current = entry.parentSubagentId;
    let value = 0;
    const visited = new Set<string>();
    while (current && parents.has(current) && !visited.has(current)) {
      visited.add(current);
      value += 1;
      current = parents.get(current) ?? undefined;
    }
    return [entry.subagentId, value] as const;
  }));
}

function statusGlyph(status: SubagentCatalogEntrySnapshot["status"]) {
  if (status === "completed") return "✓";
  if (status === "failed" || status === "disconnected") return "!";
  if (status === "cancelled") return "×";
  return "●";
}

function statusLabel(status: SubagentCatalogEntrySnapshot["status"]) {
  if (status === "waitingForActivity") return "Starting";
  return status[0]!.toUpperCase() + status.slice(1);
}
