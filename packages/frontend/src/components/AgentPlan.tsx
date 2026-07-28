import { useState } from "react";
import { ChevronDown } from "lucide-react";
import type { AgentPlan, AgentPlanEntry, TaskStatus } from "@openaide/app-shell-contracts";

const currentPlanDisclosure = new Map<string, boolean>();
const PLAN_DISCLOSURE_STORAGE_PREFIX = "openaide.agentPlanDisclosure:";

export function AgentPlanView({
  plan,
  taskId,
  taskStatus,
}: {
  plan: AgentPlan;
  taskId: string;
  taskStatus: TaskStatus;
}) {
  const [open, setOpen] = useState(() => readAgentPlanDisclosure(taskId));
  const completed = plan.entries.filter((entry) => entry.status === "completed").length;
  const current = plan.entries.find((entry) => entry.status === "in_progress")
    ?? plan.entries.find((entry) => entry.status === "pending");

  return (
    <section className="agent-plan" data-open={open}>
      <button
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} Agent Plan`}
        className="agent-plan-heading"
        onClick={() => setOpen((value) => {
          const next = !value;
          retainAgentPlanDisclosure(taskId, next);
          return next;
        })}
        type="button"
      >
        <ChevronDown aria-hidden="true" className="agent-plan-chevron" size={15} />
        <strong>Plan</strong>
        {!open && current ? (
          <span className="agent-plan-current">
            <PlanStatusMark entry={current} taskRunning={taskStatus === "active"} />
            <span>{current.content}</span>
          </span>
        ) : null}
        <small>{completed} of {plan.entries.length} complete</small>
      </button>
      {open ? <PlanEntries entries={plan.entries} taskRunning={taskStatus === "active"} /> : null}
    </section>
  );
}

/** A completed or cleared Plan makes the next Plan a new, initially collapsed disclosure. */
export function resetAgentPlanDisclosure(taskId: string) {
  currentPlanDisclosure.delete(taskId);
  try {
    availableSessionStorage()?.removeItem(planDisclosureStorageKey(taskId));
  } catch {
    // A blocked session store only removes reload retention.
  }
}

export function CompletedPlanView({ entries }: { entries: AgentPlanEntry[] }) {
  const [open, setOpen] = useState(false);
  return (
    <section className="completed-plan-row" data-open={open}>
      <button
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} completed Plan`}
        className="completed-plan-heading"
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <ChevronDown aria-hidden="true" className="agent-plan-chevron" size={14} />
        <PlanStatusMark
          entry={{ content: "Plan completed", priority: "medium", status: "completed" }}
          taskRunning={false}
        />
        <span>Plan completed</span>
        <small>{entries.length} steps</small>
      </button>
      {open ? <PlanEntries entries={entries} taskRunning={false} /> : null}
    </section>
  );
}

function PlanEntries({
  entries,
  taskRunning,
}: {
  entries: AgentPlanEntry[];
  taskRunning: boolean;
}) {
  return (
    <ol className="agent-plan-entries">
      {entries.map((entry, index) => (
        <li data-status={entry.status} key={`${index}:${entry.content}`}>
          <PlanStatusMark entry={entry} taskRunning={taskRunning} />
          <span className="agent-plan-entry-content">{entry.content}</span>
          {entry.priority === "medium" ? null : (
            <small className="agent-plan-priority">{entry.priority === "high" ? "High" : "Low"}</small>
          )}
        </li>
      ))}
    </ol>
  );
}

function PlanStatusMark({
  entry,
  taskRunning,
}: {
  entry: AgentPlanEntry;
  taskRunning: boolean;
}) {
  const animated = entry.status === "in_progress" && taskRunning;
  const label = entry.status === "in_progress"
    ? "In progress"
    : entry.status === "completed"
      ? "Completed"
      : "Pending";
  return (
    <span
      aria-label={label}
      className="agent-plan-status"
      data-animated={animated}
      data-status={entry.status}
    >
      <i />
      <i />
      <i />
    </span>
  );
}

/** Restores per-Task disclosure across navigation and page reloads in the current tab. */
function readAgentPlanDisclosure(taskId: string) {
  try {
    const retained = availableSessionStorage()?.getItem(planDisclosureStorageKey(taskId));
    if (retained === "expanded") return true;
    if (retained === "collapsed") return false;
  } catch {
    // Live memory remains available when browser storage is blocked.
  }
  return currentPlanDisclosure.get(taskId) ?? false;
}

function retainAgentPlanDisclosure(taskId: string, open: boolean) {
  currentPlanDisclosure.set(taskId, open);
  try {
    availableSessionStorage()?.setItem(
      planDisclosureStorageKey(taskId),
      open ? "expanded" : "collapsed",
    );
  } catch {
    // Disclosure still works for this page lifetime when browser storage is blocked.
  }
}

function planDisclosureStorageKey(taskId: string) {
  return `${PLAN_DISCLOSURE_STORAGE_PREFIX}${taskId}`;
}

function availableSessionStorage(): Storage | undefined {
  try {
    return globalThis.sessionStorage;
  } catch {
    return undefined;
  }
}
