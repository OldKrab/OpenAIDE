import type { ReactNode } from "react";
import type {
  ActivityToolContent,
  ActivityToolDetails,
  ActivityToolField,
  ActivityToolInput,
} from "@openaide/app-shell-contracts";
import { postHostMessage } from "../services/hostBridge";
import {
  buildUnifiedDiff,
  displayCommand,
  hasToolInput,
  parseSearchResults,
  type SearchResult,
} from "../state/toolDetailsViewModel";
import { toolValueText } from "../state/toolDetailsShared";

export type ToolOpenPathMessage = {
  type: "tool.openPath";
  payload: { line?: number; path: string };
};

export function ToolInputDetails({ input }: { input: ActivityToolInput }) {
  const command = displayCommand(input.command);
  return (
    <div className="activity-tool-input">
      {command ? <pre className="activity-tool-command">{command}</pre> : null}
      {input.path ? (
        <div className="activity-tool-primary-path">
          <ToolPath path={input.path} />
        </div>
      ) : null}
      {input.fields?.length ? <ToolFields fields={input.fields} /> : null}
    </div>
  );
}

export function ToolMeta({ details, primaryPath }: { details: ActivityToolDetails; primaryPath?: { path: string; line?: number } }) {
  const facts: { label: string; value?: ReactNode }[] = [];
  if (primaryPath) facts.push({ label: "file", value: <ToolPath line={primaryPath.line} path={primaryPath.path} /> });
  if (details.input?.cwd) facts.push({ label: "cwd", value: <span>{details.input.cwd}</span> });
  if (details.output?.exit_code !== undefined && details.output.exit_code !== 0) {
    facts.push({ label: "exit", value: <span>{details.output.exit_code}</span> });
  }
  if (details.output?.success === false) facts.push({ label: "failed" });
  if (!facts.length) return null;
  return (
    <ul className="activity-tool-meta" aria-label="Tool facts">
      {facts.map((fact) => (
        <li key={fact.label}>
          <span>{fact.label}</span>
          {fact.value}
        </li>
      ))}
    </ul>
  );
}

export function ToolFields({ fields }: { fields: ActivityToolField[] }) {
  return (
    <dl className="activity-tool-fields activity-tool-inline-fields">
      {fields.map((field) => (
        <FragmentPair key={field.name} name={field.name} value={<ToolValue value={field.value} />} />
      ))}
    </dl>
  );
}

function FragmentPair({ name, value }: { name: string; value: ReactNode }) {
  return (
    <>
      <dt>{name}</dt>
      <dd>{value}</dd>
    </>
  );
}

function ToolValue({ value }: { value: ActivityToolField["value"] }) {
  const compact = toolValueText(value);
  if (compact !== undefined) return <>{compact}</>;
  if (value.kind === "array") {
    return (
      <ol className="activity-tool-value-list">
        {value.items.map((item, index) => <li key={index}><ToolValue value={item} /></li>)}
      </ol>
    );
  }
  if (value.kind === "object") return <ToolFields fields={value.fields} />;
  return null;
}

export function ToolContentBlock({ content }: { content: ActivityToolContent }) {
  if (content.kind === "text") return <ToolCodeBlock text={content.text} />;
  if (content.kind === "diff") {
    return (
      <div className="activity-tool-diff">
        <ToolPath path={content.path} />
        <UnifiedDiff oldText={content.old_text} newText={content.new_text} />
      </div>
    );
  }
  if (content.kind === "terminal") return <code>terminal {content.terminal_id}</code>;
  if (content.kind === "image") {
    return (
      <section className="activity-tool-media">
        <img alt="Tool output" className="activity-tool-image" src={content.data_url} />
        <small>{content.media_type}</small>
        {content.uri ? <code>{content.uri}</code> : null}
      </section>
    );
  }
  if (content.kind === "audio") {
    return (
      <section className="activity-tool-media">
        <audio aria-label="Tool audio output" className="activity-tool-audio" controls src={content.data_url} />
        <small>{content.media_type}</small>
      </section>
    );
  }
  if (content.kind === "resource") {
    return (
      <section className="activity-tool-resource">
        <strong>{content.title ?? content.name ?? content.uri}</strong>
        {content.title || content.name ? <code>{content.uri}</code> : null}
        {content.description ? <p>{content.description}</p> : null}
        {content.media_type || content.size_bytes !== undefined ? (
          <small>{[content.media_type, content.size_bytes !== undefined ? formatByteCount(content.size_bytes) : undefined].filter(Boolean).join(" · ")}</small>
        ) : null}
        {content.text ? <ToolCodeBlock text={content.text} /> : null}
      </section>
    );
  }
  return (
    <p className="activity-tool-unsupported">
      Unsupported {content.content_type.replaceAll("_", " ")}
      {content.media_type ? <small>{content.media_type}</small> : null}
      {content.uri ? <code>{content.uri}</code> : null}
    </p>
  );
}

function formatByteCount(sizeBytes: number): string {
  return `${sizeBytes.toLocaleString("en-US")} ${sizeBytes === 1 ? "byte" : "bytes"}`;
}

export function ToolPath({ className, label, line, path }: { className?: string; label?: string; line?: number; path: string }) {
  const display = label ?? (line ? `${path}:${line}` : path);
  return (
    <button
      className={className ? `activity-tool-path-link ${className}` : "activity-tool-path-link"}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        postHostMessage(toolOpenPathMessage({ line, path }));
      }}
      title="Open file"
      type="button"
    >
      {display}
    </button>
  );
}

export function toolOpenPathMessage({ line, path }: { line?: number; path: string }): ToolOpenPathMessage {
  return { type: "tool.openPath", payload: { line, path } };
}

export function ToolCodeBlock({ text, tone }: { text: string; tone?: "danger" }) {
  return <pre className={`activity-tool-code${tone ? ` ${tone}` : ""}`}>{text}</pre>;
}

export function SearchOutput({ basePath, output }: { basePath?: string; output: string }) {
  const results = parseSearchResults(output, basePath);
  if (results.length) return <SearchResults results={results} />;
  return <ToolCodeBlock text={output} />;
}

function SearchResults({ results }: { results: SearchResult[] }) {
  return (
    <ol className="activity-search-results">
      {results.slice(0, 8).map((result, index) => (
        <li key={`${result.path}:${result.line}:${index}`}>
          <ToolPath label={`${result.displayPath}:${result.line}`} line={result.line} path={result.path} />
          {result.text ? <span>{result.text}</span> : null}
        </li>
      ))}
    </ol>
  );
}

function UnifiedDiff({ oldText, newText }: { oldText?: string; newText: string }) {
  const lines = buildUnifiedDiff(oldText, newText);
  return (
    <pre className="activity-tool-diff-lines">
      {lines.map((line, index) => (
        <span className={`activity-tool-diff-line ${line.kind}`} key={index}>
          {line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " "}
          {line.text}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

export { hasToolInput };
