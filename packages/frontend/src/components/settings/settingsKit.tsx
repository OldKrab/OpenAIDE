import { ArrowLeft, Check, ChevronDown, ChevronRight, Info } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { PopupDialog, PopupMenu, PopupPanel } from "../Popup";
import { infoMatches, type InfoText, type SettingsGroup, type SettingsRow, type SettingsTone } from "./settingsModel";

/** Wraps the part of `text` the search matched, so a result explains why it is listed. */
export function mark(text: string, needle: string): ReactNode {
  const at = needle ? text.toLowerCase().indexOf(needle) : -1;
  if (at < 0) return text;
  return <>{text.slice(0, at)}<mark>{text.slice(at, at + needle.length)}</mark>{text.slice(at + needle.length)}</>;
}

export function SettingsGroupView({ group, needle = "" }: { group: SettingsGroup; needle?: string }) {
  return (
    <section className="settings-group">
      {group.title || group.actions ? (
        <header>
          {group.title ? <h2>{mark(group.title, needle)}</h2> : null}
          {group.info ? <InfoHint hit={infoMatches(group.info, needle)} info={group.info} /> : null}
          {group.actions ? <span className="settings-group-actions">{group.actions}</span> : null}
        </header>
      ) : null}
      {group.notice}
      {group.rows.length ? (
        <ul aria-label={group.title} className="settings-card">
          {group.rows.map((row) => <SettingsRowView key={row.id} needle={needle} row={row} />)}
        </ul>
      ) : group.empty ? <p className="settings-empty">{group.empty}</p> : null}
    </section>
  );
}

export function SettingsRowView({ needle = "", row }: { needle?: string; row: SettingsRow }) {
  const label = <strong>{mark(row.label, needle)}</strong>;
  const description = row.description ? <small>{mark(row.description, needle)}</small> : null;
  const leading = row.leading ? <span className="settings-row-leading">{row.leading}</span> : null;
  return (
    <li className={["settings-row", row.selected ? "selected" : "", row.below ? "stacked" : ""].filter(Boolean).join(" ")}>
      <div className="settings-row-line">
        {row.onOpen ? (
          <button aria-label={`Open ${row.label}`} className="settings-row-main" onClick={row.onOpen} type="button">
            {leading}
            <span className="settings-row-copy">{label}{description}</span>
          </button>
        ) : (
          <span className="settings-row-main">
            {leading}
            <span className="settings-row-copy">
              <span className="settings-row-label">
                {label}
                {row.info ? <InfoHint hit={infoMatches(row.info, needle)} info={row.info} /> : null}
              </span>
              {description}
            </span>
          </span>
        )}
        <span className="settings-row-end">
          {row.status ? <SettingsStatus tone={row.status.tone}>{mark(row.status.text, needle)}</SettingsStatus> : null}
          {row.control}
          {row.onOpen ? <ChevronRight aria-hidden="true" className="settings-row-chevron" size={16} /> : null}
        </span>
      </div>
      {row.below}
      {row.error ? <SettingsInlineError>{row.error}</SettingsInlineError> : null}
    </li>
  );
}

/** One info pattern for all of Settings: a quiet icon that opens a popover, or a sheet on a phone. */
export function InfoHint({ hit = false, info }: { hit?: boolean; info: InfoText }) {
  const [open, setOpen] = useState(false);
  return (
    <PopupPanel
      className="settings-info-card"
      label={info.title}
      onOpenChange={setOpen}
      open={open}
      placement="bottom-start"
      trigger={(triggerProps) => (
        <button
          {...triggerProps}
          aria-label={`About ${info.title}`}
          className={`settings-info${hit ? " hit" : ""}`}
          type="button"
        >
          <Info aria-hidden="true" size={15} strokeWidth={1.6} />
        </button>
      )}
    >
      <strong>{info.title}</strong>
      {info.body.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
    </PopupPanel>
  );
}

export function SettingsButton({
  ariaLabel,
  busy = false,
  children,
  danger = false,
  disabled = false,
  icon,
  onClick,
  title,
  type = "button",
  variant = "default",
}: {
  ariaLabel?: string;
  busy?: boolean;
  children: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  onClick?: () => void;
  title?: string;
  type?: "button" | "submit";
  variant?: "default" | "primary" | "quiet";
}) {
  return (
    <button
      aria-busy={busy || undefined}
      aria-label={ariaLabel}
      className={["settings-action", variant === "default" ? "" : variant, danger ? "danger" : ""].filter(Boolean).join(" ")}
      disabled={disabled || busy}
      onClick={onClick}
      title={title}
      type={type}
    >
      {icon}{children}
    </button>
  );
}

export function SettingsSwitch({
  checked,
  disabled = false,
  label,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      aria-checked={checked}
      aria-label={label}
      className="settings-switch"
      disabled={disabled}
      onClick={() => onChange(!checked)}
      role="switch"
      type="button"
    >
      <i aria-hidden="true" />
    </button>
  );
}

/** Status colour always travels with its words. */
export function SettingsStatus({ children, tone }: { children: ReactNode; tone: SettingsTone }) {
  return <span className={`settings-status ${tone}`}><i aria-hidden="true" />{children}</span>;
}

/** A mutually exclusive choice shown as rows: the one in use says so, the others offer to switch. */
export function SettingsChoice({
  active,
  busy = false,
  busyLabel = "Restarting…",
  disabled = false,
  label,
  onUse,
}: {
  active: boolean;
  busy?: boolean;
  busyLabel?: string;
  disabled?: boolean;
  label: string;
  onUse: () => void;
}) {
  if (active) return <span className="settings-in-use"><Check aria-hidden="true" size={14} />In use</span>;
  return <SettingsButton ariaLabel={`Use ${label}`} busy={busy} disabled={disabled} onClick={onUse}>{busy ? busyLabel : "Use"}</SettingsButton>;
}

export function SettingsKeys({ keys }: { keys: string[] }) {
  return <span className="settings-keys">{keys.map((key) => <kbd key={key}>{key}</kbd>)}</span>;
}

export function SettingsSegmented<T extends string>({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (next: T) => void;
  options: Array<{ value: T; label: string }>;
  value: T;
}) {
  return (
    <span aria-label={label} className="settings-segmented" role="radiogroup">
      {options.map((option) => (
        <button aria-checked={value === option.value} key={option.value} onClick={() => onChange(option.value)} role="radio" type="button">
          {option.label}
        </button>
      ))}
    </span>
  );
}

export function SettingsSelect({
  disabled = false,
  label,
  onChange,
  options,
  placeholder,
  value,
}: {
  disabled?: boolean;
  label: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
  placeholder?: string;
  value?: string;
}) {
  const [open, setOpen] = useState(false);
  const selected = options.find((option) => option.value === value)?.label ?? placeholder ?? value ?? "";
  return (
    <PopupMenu
      className="settings-select-menu"
      label={label}
      onOpenChange={setOpen}
      open={open}
      placement="bottom-start"
      trigger={(triggerProps) => (
        <button {...triggerProps} aria-label={label} className="settings-select" disabled={disabled} type="button">
          <span>{selected}</span>
          <ChevronDown aria-hidden="true" size={14} />
        </button>
      )}
    >
      {options.map((option) => (
        <button
          aria-checked={option.value === value}
          key={option.value}
          onClick={() => {
            setOpen(false);
            onChange(option.value);
          }}
          role="menuitemradio"
          type="button"
        >
          <span>{option.label}</span>
          {option.value === value ? <Check aria-hidden="true" size={13} /> : null}
        </button>
      ))}
    </PopupMenu>
  );
}

export function SettingsAvatar({ children, large = false }: { children: ReactNode; large?: boolean }) {
  return <span aria-hidden="true" className={`settings-avatar${large ? " large" : ""}`}>{children}</span>;
}

export function SettingsInlineError({ children }: { children: ReactNode }) {
  return <p className="settings-inline-error" role="alert">{children}</p>;
}

export function SettingsNotice({ children }: { children: ReactNode }) {
  return <p className="settings-notice" role="status">{children}</p>;
}

/** Loading, unavailable, and empty states for a whole section. */
export function SettingsBlankState({ detail, title }: { detail?: string; title: string }) {
  return (
    <div className="settings-blank">
      <strong>{title}</strong>
      {detail ? <span>{detail}</span> : null}
    </div>
  );
}

/** Returns from an item's page to the section list it was opened from. */
export function SettingsCrumb({ label, onBack }: { label: string; onBack: () => void }) {
  return (
    <button aria-label={`Back to ${label}`} className="settings-crumb" onClick={onBack} type="button">
      <ArrowLeft aria-hidden="true" size={14} />{label}
    </button>
  );
}

export function SettingsIdentity({
  end,
  icon,
  subtitle,
  title,
}: {
  end?: ReactNode;
  icon: ReactNode;
  subtitle?: ReactNode;
  title: string;
}) {
  return (
    <header className="settings-identity">
      <SettingsAvatar large>{icon}</SettingsAvatar>
      <span className="settings-identity-copy">
        <h1>{title}</h1>
        {subtitle ? <small>{subtitle}</small> : null}
      </span>
      {end ? <span className="settings-header-end">{end}</span> : null}
    </header>
  );
}

/** States what is wrong with one item and offers the next step beside it. */
export function SettingsHealth({
  children,
  role = "status",
  status,
  text,
  tone,
}: {
  children?: ReactNode;
  role?: "alert" | "status";
  status: string;
  text?: ReactNode;
  tone: SettingsTone;
}) {
  return (
    <section className={`settings-health ${tone}`} role={role}>
      <SettingsStatus tone={tone}>{status}</SettingsStatus>
      <span className="settings-health-text">{text}</span>
      {children ? <span className="settings-health-actions">{children}</span> : null}
    </section>
  );
}

export function SettingsField({ children, hint, label }: { children: ReactNode; hint?: string; label: string }) {
  return (
    <label className="settings-field">
      <span>{label}</span>
      {children}
      {hint ? <small>{hint}</small> : null}
    </label>
  );
}

/**
 * The one confirmation for destructive Settings actions: what is lost in one
 * sentence, then the action named on its button. Stays open on failure and
 * shows the reason at the button that failed.
 */
export function SettingsConfirm({
  children,
  confirmLabel,
  danger = true,
  detail,
  label,
  onClose,
  onConfirm,
  pendingLabel,
  title,
}: {
  children?: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  detail: string;
  /** Accessible name of the dialog. */
  label: string;
  onClose: () => void;
  onConfirm: () => void | Promise<void>;
  pendingLabel?: string;
  title: string;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();
  const confirm = async () => {
    setPending(true);
    setError(undefined);
    try {
      await onConfirm();
      onClose();
    } catch (cause) {
      setError(errorSentence(cause, "That did not work. Try again."));
      setPending(false);
    }
  };
  return (
    <PopupDialog className="settings-confirm" label={label} onOpenChange={(open) => { if (!open && !pending) onClose(); }} open>
      <strong>{title}</strong>
      <p>{detail}</p>
      {children}
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      <footer>
        <SettingsButton disabled={pending} onClick={onClose}>Cancel</SettingsButton>
        <SettingsButton busy={pending} danger={danger} onClick={() => void confirm()} variant={danger ? "default" : "primary"}>
          {pending ? pendingLabel ?? confirmLabel : confirmLabel}
        </SettingsButton>
      </footer>
    </PopupDialog>
  );
}

/** Shells and intents reject with either an Error or a sentence written for the user. */
export function errorSentence(cause: unknown, fallback: string) {
  if (typeof cause === "string" && cause) return cause;
  return cause instanceof Error && cause.message ? cause.message : fallback;
}
