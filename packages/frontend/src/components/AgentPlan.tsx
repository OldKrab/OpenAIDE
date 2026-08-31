import { useState, type ReactNode } from "react";
import { ChevronDown, CircleX, MoreHorizontal, X } from "lucide-react";
import type { AgentPlan, AgentPlanEntry, TaskStatus } from "@openaide/app-shell-contracts";
import { PopupMenu } from "./Popup";

const currentPlanDisclosure = new Map<string, boolean>();
const PLAN_DISCLOSURE_STORAGE_PREFIX = "openaide.agentPlanDisclosure:";

export function AgentPlanView({
  collapsible = true,
  defaultOpen = false,
  onDismiss,
  onHide,
  plan,
  taskId,
  taskStatus,
}: {
  /** Separate Plan panels can stay expanded when their container already owns visibility. */
  collapsible?: boolean;
  /** Used only until the user establishes a retained disclosure preference for this Task. */
  defaultOpen?: boolean;
  /** Removes the current Plan into Chat. Distinct from hiding a drawer. */
  onDismiss?: () => Promise<void> | void;
  /** Conceals a Plan drawer without changing App Server Plan state. */
  onHide?: () => void;
  plan: AgentPlan;
  taskId: string;
  taskStatus: TaskStatus;
}) {
  const [disclosureOpen, setDisclosureOpen] = useState(
    () => readAgentPlanDisclosure(taskId) ?? defaultOpen,
  );
  const [dismissing, setDismissing] = useState(false);
  const [entered, setEntered] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const open = collapsible ? disclosureOpen : true;
  const completed = plan.entries.filter((entry) => entry.status === "completed").length;
  const current = plan.entries.find((entry) => entry.status === "in_progress")
    ?? plan.entries.find((entry) => entry.status === "pending");

  return (
    <section
      className="agent-plan"
      data-entered={entered || undefined}
      data-open={open}
      onAnimationEnd={(event) => {
        if (event.target !== event.currentTarget) return;
        setEntered(true);
      }}
    >
      {collapsible ? (
        <button
          aria-expanded={open}
          aria-label={`${open ? "Collapse" : "Expand"} Agent Plan`}
          className="agent-plan-heading"
          onClick={() => setDisclosureOpen((value) => {
            const next = !value;
            retainAgentPlanDisclosure(taskId, next);
            return next;
          })}
          type="button"
        >
          <ChevronDown aria-hidden="true" className="agent-plan-chevron" size={15} />
          <PlanHeadingContent
            completed={completed}
            current={current}
            open={open}
            plan={plan}
            taskStatus={taskStatus}
          />
        </button>
      ) : (
        <div className="agent-plan-heading">
          <PlanHeadingContent
            completed={completed}
            current={current}
            open={open}
            plan={plan}
            taskStatus={taskStatus}
          />
        </div>
      )}
      {onDismiss || onHide ? (
        <div className="agent-plan-actions">
          {onDismiss ? (
            <PopupMenu
              className="agent-plan-menu"
              label="Plan actions"
              onOpenChange={setMenuOpen}
              open={menuOpen}
              trigger={(triggerProps) => (
                <button
                  {...triggerProps}
                  aria-label="Plan actions"
                  className="agent-plan-action"
                  data-plan-action="menu"
                  disabled={dismissing}
                  title="Plan actions"
                  type="button"
                >
                  <MoreHorizontal aria-hidden="true" size={14} />
                </button>
              )}
            >
              <button
                disabled={dismissing}
                onClick={() => {
                  setMenuOpen(false);
                  setDismissing(true);
                  void Promise.resolve(onDismiss()).finally(() => setDismissing(false));
                }}
                role="menuitem"
                type="button"
              >
                Close Plan
              </button>
            </PopupMenu>
          ) : null}
          {onHide ? (
            <button
              aria-label="Hide Plan"
              className="agent-plan-action"
              data-plan-action="hide"
              onClick={onHide}
              title="Hide Plan"
              type="button"
            >
              <X aria-hidden="true" size={14} />
            </button>
          ) : null}
        </div>
      ) : null}
      <PlanDisclosure open={open}>
        <PlanEntries entries={plan.entries} taskRunning={open && taskStatus === "active"} />
      </PlanDisclosure>
    </section>
  );
}

function PlanHeadingContent({
  completed,
  current,
  open,
  plan,
  taskStatus,
}: {
  completed: number;
  current?: AgentPlanEntry;
  open: boolean;
  plan: AgentPlan;
  taskStatus: TaskStatus;
}) {
  return <>
    <strong>Plan</strong>
    {!open && current ? (
      <span className="agent-plan-current">
        <PlanStatusMark entry={current} taskRunning={taskStatus === "active"} />
        <span>{current.content}</span>
      </span>
    ) : null}
    <small>{completed} of {plan.entries.length} complete</small>
  </>;
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
      <PlanDisclosure open={open}>
        <PlanEntries entries={entries} taskRunning={false} />
      </PlanDisclosure>
    </section>
  );
}

export function ClosedPlanView({ entries }: { entries: AgentPlanEntry[] }) {
  const [open, setOpen] = useState(false);
  return (
    <section className="completed-plan-row" data-open={open}>
      <button
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} Plan closed by user`}
        className="completed-plan-heading"
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <ChevronDown aria-hidden="true" className="agent-plan-chevron" size={14} />
        <CircleX aria-hidden="true" className="closed-plan-icon" size={14} />
        <span>Plan closed by user</span>
        <small>{entries.length} steps</small>
      </button>
      <PlanDisclosure open={open}>
        <PlanEntries entries={entries} taskRunning={false} />
      </PlanDisclosure>
    </section>
  );
}

/** Keeps disclosure content mounted so both opening and closing can animate. */
function PlanDisclosure({ children, open }: { children: ReactNode; open: boolean }) {
  return (
    <div
      aria-hidden={!open}
      className="agent-plan-disclosure"
      data-open={open}
      inert={open ? undefined : true}
    >
      <div className="agent-plan-disclosure-content">
        {children}
      </div>
    </div>
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
        <li
          data-status={entry.status}
          key={`${index}:${entry.content}`}
          style={{ ["--agent-plan-entry" as string]: String(index) }}
        >
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
  return currentPlanDisclosure.get(taskId);
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
