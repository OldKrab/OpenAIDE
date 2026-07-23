import { Check, ChevronDown } from "lucide-react";
import { forwardRef, type ForwardedRef, type ReactNode } from "react";
import type { PopupTriggerProps } from "./Popup";

export const Selector = forwardRef<HTMLButtonElement, {
  className?: string;
  describedBy?: string;
  disabled: boolean;
  icon?: ReactNode;
  label: string;
  locked: boolean;
  menuOpen: boolean;
  onClick?: () => void;
  pending?: boolean;
  popupTrigger?: PopupTriggerProps;
}>(function Selector({
  className,
  describedBy,
  disabled,
  icon,
  label,
  locked,
  menuOpen,
  onClick,
  pending = false,
  popupTrigger,
}, forwardedRef) {
  const classes = ["composer-pill", className].filter(Boolean).join(" ");
  if (locked) {
    return (
      <span
        aria-busy={pending || undefined}
        aria-describedby={describedBy}
        aria-label={pending ? `${label}, updating Agent option` : undefined}
        className={`${classes} locked${pending ? " pending" : ""}`}
        title={pending ? "Updating Agent option" : "Locked after task start"}
      >
        {icon}
        <span className="composer-pill-label">{label}</span>
        <ChevronDown aria-hidden="true" size={11} />
      </span>
    );
  }
  const { ref: popupRef, ...popupProps } = popupTrigger ?? {};
  return (
    <button
      {...popupProps}
      aria-describedby={describedBy}
      aria-expanded={menuOpen}
      className={classes}
      disabled={disabled}
      onClick={popupTrigger?.onClick ?? onClick}
      ref={(node) => assignButtonRef(node, forwardedRef, popupRef)}
      type="button"
    >
      {icon}
      <span className="composer-pill-label">{label}</span>
      <ChevronDown size={11} />
    </button>
  );
});

export const IconButton = forwardRef<HTMLButtonElement, {
  ariaLabel: string;
  className?: string;
  disabled?: boolean;
  icon: ReactNode;
  onClick?: () => void;
  popupTrigger?: PopupTriggerProps;
  pressed?: boolean;
}>(function IconButton({
  ariaLabel,
  className,
  disabled,
  icon,
  onClick,
  popupTrigger,
  pressed,
}, ref) {
  const classes = ["composer-icon-button", className].filter(Boolean).join(" ");
  const { ref: popupRef, ...popupProps } = popupTrigger ?? {};
  return (
    <button
      {...popupProps}
      ref={(node) => assignButtonRef(node, ref, popupRef)}
      aria-label={ariaLabel}
      aria-pressed={pressed}
      className={classes}
      disabled={disabled}
      onClick={popupTrigger?.onClick ?? onClick}
      type="button"
    >
      {icon}
    </button>
  );
});

function assignButtonRef(
  node: HTMLButtonElement | null,
  forwardedRef?: ForwardedRef<HTMLButtonElement>,
  popupRef?: PopupTriggerProps["ref"],
) {
  popupRef?.(node);
  if (typeof forwardedRef === "function") forwardedRef(node);
  else if (forwardedRef) forwardedRef.current = node;
}

export function Popover({
  children,
  className = "",
  label,
  role = "menu",
}: {
  children: ReactNode;
  className?: string;
  label: string;
  role?: "group" | "menu";
}) {
  return (
    <div aria-label={label} className={`composer-popover ${className}`} role={role}>
      {children}
    </div>
  );
}

export function PopoverHeader({ description, label }: { description?: string; label: string }) {
  return (
    <header className="composer-popover-header">
      <strong>{label}</strong>
      {description ? <small>{description}</small> : null}
    </header>
  );
}

export function MenuButton({
  active,
  className,
  description,
  disabled,
  endIcon,
  icon,
  label,
  onClick,
}: {
  active?: boolean;
  className?: string;
  description?: string;
  disabled?: boolean;
  endIcon?: ReactNode;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  const role = active === undefined ? "menuitem" : "menuitemradio";
  return (
    <button
      aria-checked={active}
      className={[
        className,
        active === undefined ? undefined : "composer-menu-choice",
        endIcon ? "composer-menu-choice-with-end" : undefined,
        icon ? undefined : "composer-menu-choice-iconless",
        description ? undefined : "composer-menu-choice-compact",
      ].filter(Boolean).join(" ") || undefined}
      disabled={disabled}
      onClick={onClick}
      role={role}
      type="button"
    >
      {icon}
      <span>
        <strong>{label}</strong>
        {description ? <small>{description}</small> : null}
      </span>
      {endIcon ? <span aria-hidden="true" className="composer-menu-end-icon">{endIcon}</span> : active === undefined ? null : (
        <span aria-hidden="true" className="composer-menu-selection">
          {active ? <Check size={13} /> : null}
        </span>
      )}
    </button>
  );
}

export function PopoverBackButton({
  ariaLabel,
  description,
  icon,
  label,
  onClick,
}: {
  ariaLabel: string;
  description?: string;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button aria-label={ariaLabel} className="composer-popover-back" onClick={onClick} type="button">
      {icon}
      <span><strong>{label}</strong>{description ? <small>{description}</small> : null}</span>
    </button>
  );
}
