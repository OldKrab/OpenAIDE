import { ShieldCheck, ShieldQuestion } from "lucide-react";
import { useState } from "react";
import type { TaskPermissionPolicy } from "@openaide/app-shell-contracts";
import { PopupMenu } from "./Popup";

const POLICY_COPY: Record<TaskPermissionPolicy, { label: string; description: string }> = {
  ask_every_time: {
    label: "Ask every time",
    description: "Show each permission request for this Task.",
  },
  auto_approve: {
    label: "Auto-approve",
    description: "Approve offered one-time permissions for this Task.",
  },
};

/** Compact Task-header control, intentionally separate from Agent-provided configuration. */
export function TaskPermissionPolicyControl({
  disabled = false,
  disabledReason,
  onChange,
  policy,
}: {
  disabled?: boolean;
  disabledReason?: string;
  onChange: (policy: TaskPermissionPolicy) => Promise<void>;
  policy: TaskPermissionPolicy;
}) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const autoApprove = policy === "auto_approve";
  const triggerLabel = autoApprove ? "Permission handling: Auto-approve on" : "Permission handling: Ask every time";

  const choose = (nextPolicy: TaskPermissionPolicy) => {
    if (pending || nextPolicy === policy) {
      setOpen(false);
      return;
    }
    setPending(true);
    setOpen(false);
    void onChange(nextPolicy).finally(() => setPending(false));
  };

  return (
    <PopupMenu
      className="task-permission-policy-menu"
      label="Permission handling"
      onOpenChange={setOpen}
      open={open}
      placement="bottom-end"
      trigger={(triggerProps) => (
        <button
          {...triggerProps}
          aria-label={triggerLabel}
          className="task-permission-policy-trigger"
          data-enabled={autoApprove || undefined}
          disabled={disabled || pending}
          title={disabled && disabledReason ? disabledReason : triggerLabel}
          type="button"
        >
          {autoApprove ? <ShieldCheck aria-hidden="true" size={15} /> : <ShieldQuestion aria-hidden="true" size={15} />}
          {autoApprove ? <span>Auto-approve on</span> : null}
        </button>
      )}
    >
      <div className="task-permission-policy-menu-heading">Permission handling</div>
      {(Object.keys(POLICY_COPY) as TaskPermissionPolicy[]).map((option) => {
        const selected = option === policy;
        return (
          <button
            aria-checked={selected}
            className="task-permission-policy-option"
            key={option}
            onClick={() => choose(option)}
            role="menuitemradio"
            type="button"
          >
            <span aria-hidden="true" className="task-permission-policy-radio" data-selected={selected || undefined} />
            <span>
              <strong>{POLICY_COPY[option].label}</strong>
              <small>{POLICY_COPY[option].description}</small>
            </span>
          </button>
        );
      })}
    </PopupMenu>
  );
}
