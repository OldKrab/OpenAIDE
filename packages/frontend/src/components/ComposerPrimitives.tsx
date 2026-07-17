import { Check, ChevronDown, LoaderCircle } from "lucide-react";
import { forwardRef, type ReactNode } from "react";

export function Selector({
  className,
  disabled,
  icon,
  label,
  locked,
  menuOpen,
  onClick,
  pending = false,
}: {
  className?: string;
  disabled: boolean;
  icon: ReactNode;
  label: string;
  locked: boolean;
  menuOpen: boolean;
  onClick: () => void;
  pending?: boolean;
}) {
  const classes = ["composer-pill", className].filter(Boolean).join(" ");
  if (locked) {
    return (
      <span
        aria-busy={pending || undefined}
        aria-label={pending ? `${label}, updating Agent option` : undefined}
        className={`${classes} locked${pending ? " pending" : ""}`}
        title={pending ? "Updating Agent option" : "Locked after task start"}
      >
        {icon}
        {label}
        {pending ? <LoaderCircle aria-hidden="true" className="composer-config-pending" size={12} /> : null}
      </span>
    );
  }
  return (
    <button
      aria-expanded={menuOpen}
      className={classes}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {icon}
      {label}
      <ChevronDown size={11} />
    </button>
  );
}

export const IconButton = forwardRef<HTMLButtonElement, {
  ariaLabel: string;
  className?: string;
  disabled?: boolean;
  icon: ReactNode;
  onClick: () => void;
  pressed?: boolean;
}>(function IconButton({
  ariaLabel,
  className,
  disabled,
  icon,
  onClick,
  pressed,
}, ref) {
  const classes = ["composer-icon-button", className].filter(Boolean).join(" ");
  return (
    <button ref={ref} aria-label={ariaLabel} aria-pressed={pressed} className={classes} disabled={disabled} onClick={onClick} type="button">
      {icon}
    </button>
  );
});

export function Popover({
  children,
  className = "",
  label,
}: {
  children: ReactNode;
  className?: string;
  label: string;
}) {
  return (
    <div aria-label={label} className={`composer-popover ${className}`} role="menu">
      {children}
    </div>
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
  icon,
  label,
  onClick,
}: {
  ariaLabel: string;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button aria-label={ariaLabel} className="composer-popover-back" onClick={onClick} type="button">
      {icon}
      <span>{label}</span>
    </button>
  );
}
