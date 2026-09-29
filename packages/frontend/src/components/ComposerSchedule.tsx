import { Clock } from "lucide-react";
import { useId, useState } from "react";
import { IconButton } from "./ComposerPrimitives";
import { PopupPanel } from "./Popup";

/** Local wall time is converted once; App Server owns the durable UTC deadline. */
export function ComposerSchedule({ disabled, onSchedule }: {
  disabled: boolean;
  onSchedule: (notBefore: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string>();
  const id = useId();
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return (
    <PopupPanel
      className="composer-schedule"
      label="Send later"
      onOpenChange={(next) => {
        if (next) {
          setValue(localDateTime(new Date(Math.ceil((Date.now() + 5 * 60_000) / 60_000) * 60_000)));
          setError(undefined);
        }
        setOpen(next);
      }}
      open={open && !disabled}
      placement="top-end"
      trigger={(popupTrigger) => (
        <IconButton ariaLabel="Send later" disabled={disabled} icon={<Clock size={15} />}
          popupTrigger={popupTrigger} title="Send later" />
      )}
    >
      <form onSubmit={(event) => {
        event.preventDefault();
        const timestamp = new Date(value).getTime();
        if (!Number.isFinite(timestamp) || timestamp <= Date.now()
          || localDateTime(new Date(timestamp)) !== value) {
          setError("Choose a future date and time.");
          return;
        }
        if (disabled) return;
        onSchedule(String(timestamp));
        setOpen(false);
      }}>
        <label htmlFor={id}>Send later</label>
        <input aria-describedby={`${id}-help${error ? ` ${id}-error` : ""}`}
          aria-invalid={Boolean(error)} id={id} type="datetime-local" required
          value={value} onChange={(event) => { setValue(event.target.value); setError(undefined); }} />
        <p id={`${id}-help`}>{timezone}. Sends in queue order when the agent is idle.
          Keep OpenAIDE open; restarting pauses the queue.</p>
        {error ? <p className="composer-schedule-error" id={`${id}-error`} role="alert">{error}</p> : null}
        <button className="composer-schedule-submit" disabled={disabled || !value} type="submit">Schedule message</button>
      </form>
    </PopupPanel>
  );
}

function localDateTime(date: Date): string {
  const pad = (number: number) => String(number).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
