import { useEffect, useState } from "react";
import type { Ref } from "react";
import { ListTodo, Menu, MoreHorizontal, X } from "lucide-react";
import type { TaskPermissionPolicy } from "@openaide/app-shell-contracts";
import { TaskPermissionPolicyControl } from "./TaskPermissionPolicyControl";

/**
 * The phone's Task bar. Title and status get the width; only the frequent actions stay in the
 * bar: Project files (portaled into `onActionsTarget`) and the Plan when the Task has one. The
 * rest (Terminal, jump to a message, Subagents, permission handling) portal into a second row
 * that "More" opens below the bar. The row stays mounted while closed so the portaled controls
 * keep their state, and it is a row rather than a popup so their own menus anchor normally.
 */
export function MobileWorkbenchBar({
  coverTitle,
  navigationButtonRef,
  navigationOpen,
  onActionsTarget,
  onOverflowTarget,
  onToggleNavigation,
  permission,
  plan,
  subtitle,
  taskKey,
  title,
}: {
  /** A connection page's title; it replaces the Task and hides every Task control. */
  coverTitle?: string;
  navigationButtonRef: Ref<HTMLButtonElement>;
  navigationOpen: boolean;
  onActionsTarget(element: HTMLElement | null): void;
  onOverflowTarget(element: HTMLElement | null): void;
  onToggleNavigation(): void;
  permission?: {
    disabled: boolean;
    disabledReason?: string;
    onChange(policy: TaskPermissionPolicy): Promise<void>;
    policy: TaskPermissionPolicy;
  };
  plan?: { open: boolean; onToggle(): void };
  subtitle?: string;
  /** The Task shown; another Task starts with the row closed. */
  taskKey?: string;
  title?: string;
}) {
  const [moreOpen, setMoreOpen] = useState(false);
  useEffect(() => setMoreOpen(false), [taskKey]);
  const taskControls = !coverTitle && Boolean(permission);
  const showMore = taskControls && moreOpen;

  return (
    <header
      className="mobile-workbench-bar"
      data-has-permission-policy={taskControls || undefined}
      data-has-plan={plan && !coverTitle ? true : undefined}
      data-more-open={showMore || undefined}
    >
      <button
        aria-expanded={navigationOpen}
        aria-label={navigationOpen ? "Close task navigation" : "Open task navigation"}
        onClick={onToggleNavigation}
        ref={navigationButtonRef}
        type="button"
      >
        {navigationOpen ? <X size={17} /> : <Menu size={17} />}
      </button>
      <span>
        <strong>{coverTitle ?? title}</strong>
        {coverTitle ? null : <small>{subtitle}</small>}
      </span>
      <div className="mobile-task-file-action" ref={onActionsTarget} />
      {plan && !coverTitle ? (
        <button
          aria-expanded={plan.open}
          aria-label={plan.open ? "Hide Plan" : "Open Plan"}
          className="task-plan-appear-chip"
          onClick={plan.onToggle}
          type="button"
        >
          {plan.open ? <X size={17} /> : <ListTodo size={17} />}
        </button>
      ) : null}
      {taskControls ? (
        <button
          aria-controls="mobile-task-more"
          aria-expanded={showMore}
          aria-label={showMore ? "Hide more Task actions" : "More Task actions"}
          className="mobile-task-more-trigger"
          onClick={() => setMoreOpen((open) => !open)}
          type="button"
        >
          {showMore ? <X size={17} /> : <MoreHorizontal size={17} />}
        </button>
      ) : null}
      <div className="mobile-task-more" hidden={!showMore} id="mobile-task-more">
        <div className="mobile-task-more-actions" ref={onOverflowTarget} />
        {permission && taskControls ? (
          <TaskPermissionPolicyControl
            disabled={permission.disabled}
            disabledReason={permission.disabledReason}
            onChange={permission.onChange}
            policy={permission.policy}
          />
        ) : null}
      </div>
    </header>
  );
}
