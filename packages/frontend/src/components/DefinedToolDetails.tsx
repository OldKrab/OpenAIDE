import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { AgentMarkdown } from "./AgentMarkdown";
import {
  ToolCodeBlock,
  ToolContentBlock,
  ToolFields,
  ToolInputDetails,
  ToolMeta,
} from "./ChatToolBlocks";
import {
  filteredOutputFields,
  firstToolPath,
  hasToolInput,
  hasToolOutputBody,
  primaryOutput,
} from "../state/toolDetailsViewModel";

const headings: Record<string, string> = {
  delete: "Deleted item",
  move: "Moved item",
  think: "Reasoning",
  fetch: "Fetched resource",
  switch_mode: "Mode change",
};

/** Field-driven presentation for ACP kinds that do not have editor-specific views. */
export function DefinedToolDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const outputText = primaryOutput(details.output);
  const outputFields = filteredOutputFields(details);
  const thoughtIndex = step.name === "think"
    ? details.content.findIndex((item) => item.kind === "text")
    : -1;
  const thoughtContent = thoughtIndex >= 0 ? details.content[thoughtIndex] : undefined;
  const thought = thoughtContent?.kind === "text" ? thoughtContent.text : undefined;
  const content = thoughtIndex >= 0
    ? details.content.filter((_, index) => index !== thoughtIndex)
    : details.content;

  return (
    <div className={`activity-tool-details activity-tool-${step.name}-detail`}>
      <h4 className="activity-tool-detail-title">{headings[step.name]}</h4>
      <ToolMeta details={details} primaryPath={firstToolPath(details)} />
      {details.input && hasToolInput(details.input) ? <ToolInputDetails input={details.input} /> : null}
      {thought ? <AgentMarkdown className="chat-thought activity-tool-reasoning" text={thought} /> : null}
      {content.length ? (
        <section className="activity-tool-section">
          {content.map((item, index) => (
            <ToolContentBlock content={item} key={index} />
          ))}
        </section>
      ) : null}
      {details.output && hasToolOutputBody(details.output) ? (
        <section className="activity-tool-section">
          {outputText ? <ToolCodeBlock text={outputText} /> : null}
          {details.output.stderr ? <ToolCodeBlock text={details.output.stderr} tone="danger" /> : null}
          {outputFields.length ? <ToolFields fields={outputFields} /> : null}
        </section>
      ) : fallbackPreview ? <ToolCodeBlock text={fallbackPreview} /> : null}
    </div>
  );
}
