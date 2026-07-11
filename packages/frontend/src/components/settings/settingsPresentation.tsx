import { AlertTriangle, CheckCircle2, CircleOff } from "lucide-react";

export function StatusBadge({ status }: { status: string }) {
  const failed = status === "failed" || status === "invalid";
  const ready = status === "connected" || status === "ready" || status === "valid" || status === "available";
  const Icon = failed ? AlertTriangle : ready ? CheckCircle2 : CircleOff;
  return (
    <span className={`settings-status ${status}`}>
      <Icon size={12} />
      {label(status)}
    </span>
  );
}

export function InlineFailure({ message, muted = false }: { message: string; muted?: boolean }) {
  return (
    <details className={`settings-inline-message ${muted ? "muted" : "error"}`}>
      <summary>{muted ? "Warning details" : "Failure details"}</summary>
      <code>{message}</code>
    </details>
  );
}

export function InlineNotice({ message }: { message: string }) {
  return <p className="settings-notice">{message}</p>;
}

export function EmptySettingsState({ detail, title }: { detail: string; title: string }) {
  return (
    <div className="settings-empty">
      <strong>{title}</strong>
      <span>{detail}</span>
    </div>
  );
}

export function SettingsSkeleton() {
  return (
    <div className="settings-panel" aria-label="Loading settings">
      <div className="settings-empty">
        <strong>Loading settings</strong>
        <span>Refreshing this section.</span>
      </div>
      <div className="settings-skeleton" />
      <div className="settings-skeleton short" />
      <div className="settings-skeleton" />
    </div>
  );
}

function label(value: string) {
  return value
    .split(/[_-]/)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}
