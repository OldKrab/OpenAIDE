import { Check, X } from "lucide-react";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import type { EditDetailLine } from "../state/toolDetailsViewModel";
import { editDiffLines, editResultText, firstDiffContent, firstToolPath } from "../state/toolDetailsViewModel";
import { ToolCodeBlock, ToolPath } from "./ChatToolBlocks";

export function EditToolDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const diff = firstDiffContent(details);
  const fullPath = diff?.path ?? firstToolPath(details)?.path ?? details.input?.path ?? "";
  const lines = diff ? editDiffLines(diff) : [];
  const failed = step.status === "error" || details.output?.success === false;
  const result = editResultText(details, fullPath, failed, fallbackPreview);
  return (
    <div className="activity-tool-details activity-tool-edit-detail">
      {fullPath ? (
        <dl className="edit-tool-meta">
          <dt>file</dt>
          <dd>
            <ToolPath path={fullPath} />
          </dd>
        </dl>
      ) : null}
      {lines.length ? (
        <pre className="edit-tool-diff">
          {lines.map((line, index) => <EditDiffRow key={`${index}-${line.kind}-${line.text}`} line={line} />)}
        </pre>
      ) : fallbackPreview ? (
        <ToolCodeBlock text={fallbackPreview} />
      ) : (
        <p className="activity-tool-muted">No diff content returned.</p>
      )}
      <p className={`edit-tool-result ${failed ? "failed" : "applied"}`}>
        {failed ? <X size={14} aria-hidden="true" /> : <Check size={14} aria-hidden="true" />}
        {result}
      </p>
    </div>
  );
}

function EditDiffRow({ line }: { line: EditDetailLine }) {
  if (line.kind === "hunk") {
    return (
      <span className="edit-tool-hunk-row">
        <span className="edit-tool-hunk-label">{line.text}</span>
      </span>
    );
  }
  if (line.kind === "omitted") {
    return (
      <span className="edit-tool-omitted-row">
        <span className="edit-tool-omitted-label">{line.text}</span>
      </span>
    );
  }
  return (
    <span className={`edit-tool-line ${line.kind}`}>
      <span aria-hidden="true" className="edit-tool-old-line-number">{line.oldLineNumber ?? ""}</span>
      <span aria-hidden="true" className="edit-tool-new-line-number">{line.newLineNumber ?? ""}</span>
      <span aria-hidden="true" className="edit-tool-prefix">{line.prefix}</span>
      <span className="edit-tool-line-text">{line.text || " "}</span>
    </span>
  );
}
