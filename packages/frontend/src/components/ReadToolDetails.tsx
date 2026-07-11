import { Terminal } from "lucide-react";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { displayCommand, readDetailOutput, readDetailPath, splitToolLines } from "../state/toolDetailsViewModel";

export function ReadToolDetails({
  details,
  fallbackPreview,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const fullPath = readDetailPath(details);
  const output = readDetailOutput(details, fallbackPreview);
  const command = displayCommand(details.input?.command);
  const lines = output ? splitToolLines(output) : [];
  return (
    <div className="activity-tool-details activity-tool-read-detail">
      {fullPath ? <p className="read-tool-path">{fullPath}</p> : null}
      {lines.length ? (
        <pre className="read-tool-output">
          {lines.map((line, index) => (
            <span className="read-tool-line" key={`${index}-${line}`}>
              <span className="read-tool-line-number">{index + 1}</span>
              <span className="read-tool-line-text">{line || " "}</span>
            </span>
          ))}
        </pre>
      ) : (
        <p className="activity-tool-muted">No output returned.</p>
      )}
      {command ? (
        <div className="read-tool-command" aria-label="Read command">
          <Terminal size={14} aria-hidden="true" />
          <code>{command}</code>
        </div>
      ) : null}
    </div>
  );
}
