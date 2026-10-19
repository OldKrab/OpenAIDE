import { ChevronDown } from "lucide-react";
import { useState } from "react";
import type { SubagentCatalogEntrySnapshot, SubagentId } from "@openaide/app-server-client";

import { PopupPanel } from "./Popup";

/**
 * Chrome of a selected Subagent History: which history is shown, its Agent-specific
 * details, Stop when the Agent can stop it, and the one-action return to Task Chat. App
 * Server owns which details exist, their labels, and the Subagent's status; this
 * component renders them as given.
 */
export function SubagentInspectionBar({
  onReturn,
  onStop,
  selected,
}: {
  onReturn: () => void;
  onStop?: (subagentId: SubagentId) => Promise<boolean>;
  selected: SubagentCatalogEntrySnapshot;
}) {
  const stoppable = onStop !== undefined
    && selected.capabilities.cancel
    && (selected.status === "running" || selected.status === "waitingForActivity");
  return (
    <div className="subagent-inspection-footer" role="note">
      <span className="subagent-inspection-context">
        <span className="subagent-inspection-name">
          Viewing <strong>{selected.name}</strong>
        </span>
        {/* Keyed so an open disclosure never carries over to another Subagent. */}
        <SubagentDetails details={selected.details ?? []} key={selected.subagentId} />
      </span>
      {stoppable ? (
        // Keyed so a pending stop ends when the reported status or the Subagent changes.
        <SubagentStopButton
          key={`${selected.subagentId}:${selected.status}`}
          name={selected.name}
          onStop={() => onStop(selected.subagentId)}
        />
      ) : null}
      <button
        aria-keyshortcuts="Alt+ArrowLeft"
        className="subagent-inspection-return"
        onClick={onReturn}
        title="Back to Main Agent (Alt+Left)"
        type="button"
      >
        Back to Main Agent
      </button>
    </div>
  );
}

/**
 * Stays pending after an accepted request: only the Agent's report, arriving as a new
 * status, ends the stop. A rejected request re-enables it for a retry.
 */
function SubagentStopButton({ name, onStop }: { name: string; onStop: () => Promise<boolean> }) {
  const [pending, setPending] = useState(false);
  return (
    <button
      aria-label={`Stop Subagent: ${name}`}
      className="subagent-inspection-stop"
      disabled={pending}
      onClick={() => {
        setPending(true);
        void onStop().then((accepted) => {
          if (!accepted) setPending(false);
        });
      }}
      type="button"
    >
      {pending ? "Stopping…" : "Stop"}
    </button>
  );
}

/**
 * Compact disclosure whose trigger already reads as the detail values, so the common
 * case (Agent type and model) needs no click while long values stay out of the bar.
 */
function SubagentDetails({ details }: { details: NonNullable<SubagentCatalogEntrySnapshot["details"]> }) {
  const [open, setOpen] = useState(false);
  if (details.length === 0) return null;
  const spoken = details.map((detail) => `${detail.label} ${detail.value}`).join(", ");
  return (
    <PopupPanel
      className="subagent-details-popup"
      label="Subagent details"
      onOpenChange={setOpen}
      open={open}
      placement="top-start"
      trigger={(props) => (
        <button
          {...props}
          aria-label={`Subagent details: ${spoken}`}
          className="subagent-details-trigger"
          title="Subagent details"
          type="button"
        >
          <span className="subagent-details-summary">
            {details.map((detail) => detail.value).join(" · ")}
          </span>
          <span className="subagent-details-compact-label">Details</span>
          <ChevronDown aria-hidden="true" className="subagent-details-chevron" size={12} />
        </button>
      )}
    >
      <dl className="subagent-details-list">
        {details.map((detail) => (
          <div key={detail.label}>
            <dt>{detail.label}</dt>
            <dd>{detail.value}</dd>
          </div>
        ))}
      </dl>
    </PopupPanel>
  );
}
