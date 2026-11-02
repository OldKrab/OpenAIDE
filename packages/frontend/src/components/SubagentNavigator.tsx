import { Check, ChevronDown, Users } from "lucide-react";
import { useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { SubagentCatalogEntrySnapshot } from "@openaide/app-server-client";
import type { AgentIconId } from "@openaide/app-shell-contracts";

import { AgentIcon } from "./AgentIcon";
import { MobileTaskMoreSubmenu, useMobileTaskMore } from "./MobileTaskMore";
import { PopupMenu } from "./Popup";
import { subagentTreeRows } from "./subagentTree";

export function SubagentNavigator({
  agentIcon,
  agentId,
  agentName,
  entries,
  onSelect,
  selectedId,
  target,
  unseen,
}: {
  /** Identity of the Task's own Agent, shown while the navigator is on its main history. */
  agentIcon?: AgentIconId;
  agentId?: string;
  agentName?: string;
  entries: SubagentCatalogEntrySnapshot[];
  onSelect: (subagentId?: string) => void;
  selectedId?: string;
  /** The phone's More menu, where the navigator is a submenu instead of a popup. */
  target?: HTMLElement | null;
  unseen: Set<string>;
}) {
  const [open, setOpen] = useState(false);
  const more = useMobileTaskMore();
  if (entries.length === 0) return null;
  const selected = entries.find((entry) => entry.subagentId === selectedId);
  const selectedLabel = selected ? subagentLabel(selected) : "Main Agent";
  const choices = (close: () => void) => (
    <SubagentChoices
      agentIcon={agentIcon}
      agentId={agentId}
      agentName={agentName}
      entries={entries}
      onSelect={(subagentId) => {
        close();
        onSelect(subagentId);
      }}
      selectedId={selectedId}
      unseen={unseen}
    />
  );

  if (target) {
    const hasUnseen = entries.some((entry) => unseen.has(entry.subagentId));
    const running = entries.filter(({ status }) => status === "running" || status === "waitingForActivity").length;
    return createPortal(
      <MobileTaskMoreSubmenu
        aria-label={`Switch agent. Currently viewing ${selectedLabel}${hasUnseen ? ". New activity" : ""}`}
        end={hasUnseen ? <em>New</em> : undefined}
        icon={<Users aria-hidden="true" />}
        label="Agents"
        meta={running > 0 ? `${running} running` : undefined}
        slot="agents"
        value={selectedLabel}
      >
        <div aria-label="Switch agent" className="subagent-tree" role="menu">{choices(more.close)}</div>
      </MobileTaskMoreSubmenu>,
      target,
    );
  }

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
            aria-label={`Switch agent. Currently viewing ${selectedLabel}`}
            className="subagent-switcher-trigger"
            title={`Switch agent history. Currently viewing ${selectedLabel}`}
            type="button"
          >
            {selected ? (
              <span className={`subagent-status-dot ${selected.status}`} aria-hidden="true" />
            ) : (
              // The main history belongs to the Task's Agent, so it carries that Agent's
              // configured icon rather than a generic one.
              <AgentIcon agentId={agentId} agentName={agentName} icon={agentIcon} size={13} />
            )}
            <span className="subagent-switcher-value">{selectedLabel}</span>
            {!selected ? <span className="subagent-count" aria-label={`${entries.length} subagents`}>{entries.length}</span> : null}
            <ChevronDown aria-hidden="true" className="subagent-switcher-chevron" size={13} />
          </button>
        )}
      >
        <div className="subagent-menu-heading">
          <strong>Agents</strong>
          <span>{entries.length + 1} histories</span>
        </div>
        {choices(() => setOpen(false))}
      </PopupMenu>
    </div>
  );
}

/** Main Agent and every Subagent; the popup and the phone's More menu both list these. */
function SubagentChoices({
  agentIcon,
  agentId,
  agentName,
  entries,
  onSelect,
  selectedId,
  unseen,
}: Pick<Parameters<typeof SubagentNavigator>[0], "agentIcon" | "agentId" | "agentName" | "entries" | "onSelect" | "selectedId" | "unseen">) {
  const selected = entries.some((entry) => entry.subagentId === selectedId);
  return (
    <>
      <button
        aria-checked={!selected}
        className="subagent-menu-item"
        onClick={() => onSelect(undefined)}
        role="menuitemradio"
        type="button"
      >
        <span className="subagent-menu-icon">
          <AgentIcon agentId={agentId} agentName={agentName} icon={agentIcon} size={14} />
        </span>
        <span className="subagent-menu-copy">
          <strong>Main Agent</strong>
          <small>Primary conversation</small>
        </span>
        {!selected ? <Check aria-hidden="true" className="subagent-menu-check" size={14} /> : null}
      </button>
      <div className="subagent-menu-separator" role="separator" />
      {subagentTreeRows(entries).map(({ depth, entry, guides }) => {
        const label = subagentLabel(entry);
        return (
          <button
            aria-checked={entry.subagentId === selectedId}
            className="subagent-menu-item"
            key={entry.subagentId}
            onClick={() => onSelect(entry.subagentId)}
            role="menuitemradio"
            style={{ "--subagent-depth": depth } as CSSProperties}
            type="button"
          >
            {guides.length > 0 ? (
              <span aria-hidden="true" className="subagent-tree-guides">
                {guides.map(({ level, part }) => (
                  <i data-part={part} key={`${level}-${part}`} style={{ "--subagent-guide-level": level } as CSSProperties} />
                ))}
              </span>
            ) : null}
            <span className="subagent-menu-icon">
              <span className={`subagent-status-dot ${entry.status}`} aria-hidden="true" />
            </span>
            <span className="subagent-menu-copy">
              <span className="subagent-menu-name">
                <strong title={label}>{label}</strong>
                <span>{statusLabel(entry.status)}</span>
                {unseen.has(entry.subagentId) ? <em>New</em> : null}
              </span>
              {label === entry.name ? null : <small>{entry.name}</small>}
            </span>
            {entry.subagentId === selectedId ? <Check aria-hidden="true" className="subagent-menu-check" size={14} /> : null}
          </button>
        );
      })}
    </>
  );
}

/**
 * What a Subagent is called in navigation. An Agent's name is often its Subagent type
 * ("fork", "general-purpose"), shared by every child of that type, so the delegated task
 * the Agent gave it leads and the name is the fallback.
 */
export function subagentLabel(entry: Pick<SubagentCatalogEntrySnapshot, "delegatedTask" | "name">) {
  return entry.delegatedTask.trim() || entry.name;
}

function statusLabel(status: SubagentCatalogEntrySnapshot["status"]) {
  if (status === "waitingForActivity") return "Starting";
  return status[0]!.toUpperCase() + status.slice(1);
}
