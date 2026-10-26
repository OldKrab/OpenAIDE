import type { ReactNode } from "react";
import { AppWindow, Monitor, Smartphone, X } from "lucide-react";

/** One end of the link: the device in hand, or the place it uses. */
export type ConnectionEnd = {
  label: string;
  kind: "phone" | "computer" | "app";
  /** A version or "Update needed" under the label; `stale` marks the side to update. */
  note?: string;
  stale?: boolean;
};

/**
 * The layout every can't-connect and switching page shares (DESIGN.md: one focal point, the
 * real objects instead of prose): a picture of the link, a title, one line, and the actions.
 * On a phone the actions sit at the bottom, within thumb reach; on wider windows they follow
 * the line. `children` holds what comes after the actions there: other places and Details.
 */
export function ConnectionPage({ actions, busy, children, here, label, line, link, there, title }: {
  actions?: ReactNode;
  busy?: boolean;
  children?: ReactNode;
  here: ConnectionEnd;
  label: string;
  line: ReactNode;
  link: "broken" | "waiting" | "mismatch";
  there: ConnectionEnd;
  title: string;
}) {
  return (
    <section aria-busy={busy || undefined} aria-label={label} aria-live="polite" className="task-surface connection-page">
      <div aria-hidden="true" className={`connection-link connection-link-${link}`}>
        <LinkEnd end={here} />
        <div className="connection-wire">
          <span className="connection-wire-line" />
          {link === "broken" ? <span className="connection-wire-mark"><X size={12} strokeWidth={2.2} /></span> : null}
          {link === "mismatch" ? <span className="connection-wire-mark connection-wire-mark-warning">≠</span> : null}
          {link === "waiting" ? <span className="connection-wire-dots"><i /><i /><i /></span> : null}
        </div>
        <LinkEnd end={there} />
      </div>
      <h2>{title}</h2>
      <p>{line}</p>
      {actions ? <div className="connection-page-actions">{actions}</div> : null}
      {children}
    </section>
  );
}

function LinkEnd({ end }: { end: ConnectionEnd }) {
  const Icon = end.kind === "phone" ? Smartphone : end.kind === "app" ? AppWindow : Monitor;
  return (
    <div className="connection-end">
      <Icon size={30} strokeWidth={1.4} />
      <span className="connection-end-label">{end.label}</span>
      {end.note ? <span className={`connection-end-note${end.stale ? " stale" : ""}`}>{end.note}</span> : null}
    </div>
  );
}

/** The device this Frontend runs on, as the near end of the picture. */
export function thisDevice(device: "phone" | "computer" | undefined): ConnectionEnd {
  return device === "phone" ? { label: "This phone", kind: "phone" } : { label: "This computer", kind: "computer" };
}
