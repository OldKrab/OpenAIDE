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
        <FragmentPair key={field.name} name={field.name} value={field.value} />
      ))}
    </dl>
  );
}

function FragmentPair({ name, value }: { name: string; value: string }) {
  return (
    <>
      <dt>{name}</dt>
      <dd>{value}</dd>
    </>
  );
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
  return <span>{content.label}</span>;
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
