import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { filteredOutputFields, firstToolPath, hasToolOutputBody, primaryOutput } from "../state/toolDetailsViewModel";
import { hasToolInput, SearchOutput, ToolCodeBlock, ToolContentBlock, ToolFields, ToolInputDetails, ToolMeta } from "./ChatToolBlocks";

export function GenericToolStepDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const outputText = primaryOutput(details.output);
  const primaryPath = firstToolPath(details);
  const outputFields = filteredOutputFields(details);
  return (
    <div className="activity-tool-details">
      <ToolMeta details={details} primaryPath={primaryPath} />
      {details.input && hasToolInput(details.input) ? <ToolInputDetails input={details.input} /> : null}
      {details.content?.length ? (
        <section className="activity-tool-section">
          {details.content.map((content, index) => (
            <ToolContentBlock content={content} key={index} />
          ))}
        </section>
      ) : null}
      {details.output && hasToolOutputBody(details.output) ? (
        <section className="activity-tool-section">
          {step.name === "search" && outputText ? <SearchOutput basePath={details.input?.cwd} output={outputText} /> : null}
          {step.name !== "search" && outputText ? <ToolCodeBlock text={outputText} /> : null}
          {details.output.stderr ? (
            <>
              <span className="activity-tool-subtitle">stderr</span>
              <ToolCodeBlock tone="danger" text={details.output.stderr} />
            </>
          ) : null}
          {outputFields.length ? <ToolFields fields={outputFields} /> : null}
        </section>
      ) : fallbackPreview ? (
        <ToolCodeBlock text={fallbackPreview} />
      ) : null}
    </div>
  );
}
