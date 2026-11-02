import { useEffect, useRef } from "react";
import type { Ref, RefObject } from "react";
import { ListTodo, Menu, MoreHorizontal, X } from "lucide-react";
import type { TaskPermissionPolicy } from "@openaide/app-shell-contracts";
import { useMobileTaskMore } from "./MobileTaskMore";
import { TaskPermissionSwitch } from "./TaskPermissionPolicyControl";
import { useBackNavigation } from "./useBackNavigation";

/**
 * The phone's Task bar. Title and status get the width; only the frequent actions stay in the
 * bar: Project files (portaled into `onActionsTarget`) and the Plan when the Task has one. The
 * rest (Terminal, Subagents, jump to a message, permission handling) wait in a menu that "More"
 * opens under its button. The menu stays mounted while closed so the portaled controls keep their
 * state; a submenu replaces its content in place, so nothing stacks. See `useMoreDismissal`.
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
  const more = useMobileTaskMore();
  const { close } = more;
  useEffect(() => close(), [close, taskKey]);
  const taskControls = !coverTitle && Boolean(permission);
  const showMore = taskControls && more.open;
  const triggerRef = useRef<HTMLButtonElement>(null);
  useMoreDismissal(showMore, triggerRef);

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
          onClick={more.toggle}
          ref={triggerRef}
          type="button"
        >
          {showMore ? <X size={17} /> : <MoreHorizontal size={17} />}
        </button>
      ) : null}
      {showMore ? (
        // A press outside lands here, so the same press cannot also act on what is underneath.
        <button aria-hidden="true" className="mobile-task-more-scrim" onClick={close} tabIndex={-1} type="button" />
      ) : null}
      <div
        className="mobile-task-more"
        data-motion={more.motion}
        data-submenu={more.submenu}
        hidden={!showMore}
        id="mobile-task-more"
        role="group"
        aria-label="Task actions"
      >
        <div className="mobile-task-more-actions" ref={onOverflowTarget} />
        {permission && taskControls ? (
          <TaskPermissionSwitch
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

/** What counts as inside the open menu: the menu, its trigger, and the scrim, which closes it itself. */
const MORE_PARTS = "#mobile-task-more, .mobile-task-more-trigger, .mobile-task-more-scrim";

/**
 * Back steps out one level. A press on the rest of the bar and Escape close the whole menu; a press
 * elsewhere reaches the scrim. Rows that ran an action close the menu themselves.
 */
function useMoreDismissal(open: boolean, triggerRef: RefObject<HTMLButtonElement | null>) {
  const { close, closeSubmenu, submenu } = useMobileTaskMore();
  useBackNavigation(open, () => (submenu ? closeSubmenu() : close()));
  useEffect(() => {
    if (!open || typeof document === "undefined") return;
    const onPointerDown = (event: PointerEvent) => {
      if (!(event.target instanceof Element && event.target.closest(MORE_PARTS))) close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      close();
      triggerRef.current?.focus();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [close, open, triggerRef]);
}
