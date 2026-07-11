import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { firstFieldValue, primaryOutput } from "../state/toolDetailsShared";
import { ToolCodeBlock, ToolContentBlock } from "./ChatToolBlocks";

/** Renders web-search input and results without applying workspace-search semantics. */
export function WebSearchToolDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const query =
    details.input?.query ??
    firstFieldValue(details.input?.fields, "query") ??
    firstFieldValue(details.input?.fields, "q") ??
    step.input_summary;
  const queries = details.input?.queries?.filter((item) => item.trim()) ?? [];
  const output = primaryOutput(details.output) ?? fallbackPreview;

  return (
    <div className="activity-tool-details activity-tool-web-search-detail">
      {queries.length > 1 ? (
        <ol className="web-search-tool-queries">
          {queries.map((item, index) => (
            <li key={`${item}-${index}`}>{item}</li>
          ))}
        </ol>
      ) : query ? (
        <p className="web-search-tool-query">{queries[0] ?? query}</p>
      ) : null}
      {details.content?.length ? (
        <section className="activity-tool-section">
          {details.content.map((content, index) => (
            <ToolContentBlock content={content} key={index} />
          ))}
        </section>
      ) : null}
      {output ? <ToolCodeBlock text={output} /> : null}
      {details.output?.stderr ? <ToolCodeBlock text={details.output.stderr} tone="danger" /> : null}
    </div>
  );
}
