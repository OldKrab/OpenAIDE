import { ChevronRight, CircleDashed } from "lucide-react";
import { useId, useState } from "react";
import type { BackgroundCommand } from "@openaide/app-shell-contracts";
import { elapsedDurationLabel, formatElapsedDuration, useElapsedSeconds } from "./elapsedDuration";
import { backgroundWorkLabel } from "./taskSurfaceHelpers";

/**
 * Chat row for Background Work of the active turn. It lists what the Agent left running and lets
 * the user stop one command; ending the whole turn stays with the Composer's Stop.
 */
export function BackgroundCommandsRow({
  commands,
  onStop,
}: {
  commands: BackgroundCommand[];
  onStop?: (commandId: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const listId = useId();
  return (
    <div className="background-commands">
      <div className="background-commands-summary">
        <CircleDashed aria-hidden="true" className="working-status-background-icon" size={14} />
        {commands.length === 0 ? (
          <span className="background-commands-label" role="status" aria-live="polite">
            {backgroundWorkLabel(0)}
          </span>
        ) : (
          <button
            aria-controls={listId}
            aria-expanded={expanded}
            className="background-commands-toggle"
            onClick={() => setExpanded((value) => !value)}
            type="button"
          >
            <span role="status" aria-live="polite">{backgroundWorkLabel(commands.length)}</span>
            <ChevronRight aria-hidden="true" className="background-commands-chevron" size={13} />
          </button>
        )}
      </div>
      {commands.length > 0 && expanded ? (
        <ul className="background-commands-list" id={listId}>
          {commands.map((command) => (
            <li key={command.command_id}>
              <BackgroundCommandEntry command={command} onStop={onStop} />
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function BackgroundCommandEntry({
  command,
  onStop,
}: {
  command: BackgroundCommand;
  onStop?: (commandId: string) => void;
}) {
  // A paused command is not accumulating run time worth watching.
  const elapsedSeconds = useElapsedSeconds(command.paused ? undefined : command.started_at);
  const description = command.description || "Background command";
  return (
    <span className="background-command">
      <span className="background-command-description" title={description}>{description}</span>
      {command.kind_label ? <small className="background-command-kind">{command.kind_label}</small> : null}
      {command.paused ? <small className="background-command-state">Paused</small> : null}
      {command.stop_failed ? (
        <small className="background-command-state failed" role="status">Couldn't stop</small>
      ) : null}
      {elapsedSeconds !== undefined ? (
        <time
          aria-label={`Running for ${elapsedDurationLabel(elapsedSeconds)}`}
          className="working-status-duration"
          dateTime={`PT${elapsedSeconds}S`}
        >
          {formatElapsedDuration(elapsedSeconds)}
        </time>
      ) : null}
      {command.can_stop && onStop ? (
        <button
          aria-label={`Stop background command: ${description}`}
          className="background-command-stop"
          onClick={() => onStop(command.command_id)}
          type="button"
        >
          {command.stop_failed ? "Retry stop" : "Stop"}
        </button>
      ) : null}
    </span>
  );
}
