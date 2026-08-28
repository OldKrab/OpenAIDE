import { Bot, Check, ChevronDown } from "lucide-react";
import { useState, type CSSProperties } from "react";
import type { SubagentCatalogEntrySnapshot } from "@openaide/app-server-client";

import { PopupMenu } from "./Popup";

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
  const [open, setOpen] = useState(false);
  if (entries.length === 0) return null;
  const selected = entries.find((entry) => entry.subagentId === selectedId);
  const depth = hierarchyDepths(entries);
  const choose = (subagentId?: string) => {
    setOpen(false);
    onSelect(subagentId);
  };

  return (
    <div className="subagent-navigator">
      <PopupMenu
        className="subagent-menu"
        label="Switch agent"
        onOpenChange={setOpen}
        open={open}
        placement="bottom-end"
        trigger={(props) => (
          <button
            {...props}
            aria-label={`Switch agent. Currently viewing ${selected?.name ?? "Main Agent"}`}
            className="subagent-switcher-trigger"
            title="Switch agent history"
            type="button"
          >
            {selected ? (
              <span className={`subagent-status-dot ${selected.status}`} aria-hidden="true" />
            ) : (
              <Bot aria-hidden="true" size={13} />
            )}
            <span className="subagent-switcher-value">{selected?.name ?? "Main Agent"}</span>
            {!selected ? <span className="subagent-count" aria-label={`${entries.length} subagents`}>{entries.length}</span> : null}
            <ChevronDown aria-hidden="true" className="subagent-switcher-chevron" size={13} />
          </button>
        )}
      >
        <div className="subagent-menu-heading">
          <strong>Agents</strong>
          <span>{entries.length + 1} histories</span>
        </div>
        <button
          aria-checked={!selected}
          className="subagent-menu-item"
          onClick={() => choose(undefined)}
          role="menuitemradio"
          type="button"
        >
          <span className="subagent-menu-icon"><Bot aria-hidden="true" size={14} /></span>
          <span className="subagent-menu-copy">
            <strong>Main Agent</strong>
            <small>Primary conversation</small>
          </span>
          {!selected ? <Check aria-hidden="true" className="subagent-menu-check" size={14} /> : null}
        </button>
        <div className="subagent-menu-separator" role="separator" />
        {entries.map((entry) => (
          <button
            aria-checked={entry.subagentId === selectedId}
            className="subagent-menu-item"
            key={entry.subagentId}
            onClick={() => choose(entry.subagentId)}
            role="menuitemradio"
            style={{ "--subagent-depth": depth.get(entry.subagentId) ?? 0 } as CSSProperties}
            type="button"
          >
            <span className="subagent-menu-icon">
              <span className={`subagent-status-dot ${entry.status}`} aria-hidden="true" />
            </span>
            <span className="subagent-menu-copy">
              <span className="subagent-menu-name">
                <strong>{entry.name}</strong>
                <span>{statusLabel(entry.status)}</span>
                {unseen.has(entry.subagentId) ? <em>New</em> : null}
              </span>
            </span>
            {entry.subagentId === selectedId ? <Check aria-hidden="true" className="subagent-menu-check" size={14} /> : null}
          </button>
        ))}
      </PopupMenu>
    </div>
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

function statusLabel(status: SubagentCatalogEntrySnapshot["status"]) {
  if (status === "waitingForActivity") return "Starting";
  return status[0]!.toUpperCase() + status.slice(1);
}
