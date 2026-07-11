import { CircleX } from "lucide-react";
import type { ReactNode } from "react";
import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { caretLine, cleanShellError, searchDetailInfo } from "../state/toolDetailsViewModel";
import { ToolPath } from "./ChatToolBlocks";

export function SearchToolDetails({
  details,
  fallbackPreview,
  step,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const info = searchDetailInfo(details, step, fallbackPreview);
  const failed = step.status === "error" || info.exitCode === 127 || Boolean(details.output?.stderr && !info.output);
  const noMatches = !failed && info.matchCount === 0;
  return (
    <div className={`activity-tool-details activity-tool-search-detail ${failed ? "failed" : noMatches ? "empty" : "matched"}`}>
      {failed ? (
        <div className="search-tool-failure">
          {info.command ? <code>{info.command}</code> : null}
          <p>
            <CircleX size={14} aria-hidden="true" />
            {cleanShellError(details.output?.stderr ?? info.output) || fallbackPreview || "Search failed."}
          </p>
        </div>
      ) : noMatches ? (
        <p className="search-tool-empty">
          {info.mode === "files" ? `No files matching ${info.query}` : `No matches in ${info.path}`}
        </p>
      ) : info.matches.length ? (
        <div className="search-tool-results">
          {info.matches.map((match, index) => (
            <section className="search-tool-result" key={`${match.path}-${match.lineNumber}-${index}`}>
              <ToolPath className="search-tool-file" label={match.displayPath} line={match.lineNumber} path={match.path} />
              <pre className="search-tool-code">
                <span className="search-tool-code-line">
                  <span className="search-tool-line-number">{match.lineNumber}</span>
                  <span className="search-tool-line-text">{highlightSearchText(match.text, info.query)}</span>
                </span>
                {caretLine(match.text, info.query) ? (
                  <span className="search-tool-code-line search-tool-caret-line">
                    <span className="search-tool-line-number" />
                    <span className="search-tool-line-text">{caretLine(match.text, info.query)}</span>
                  </span>
                ) : null}
              </pre>
            </section>
          ))}
        </div>
      ) : (
        <div className="search-tool-files">
          {info.fileResults.map((result) => (
            <ToolPath
              className="search-tool-file search-tool-file-row"
              key={result.path}
              label={result.displayPath}
              path={result.path}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function highlightSearchText(text: string, query: string): ReactNode[] {
  if (!query) return [text];
  const index = text.toLowerCase().indexOf(query.toLowerCase());
  if (index < 0) return [text];
  return [
    text.slice(0, index),
    <mark key="match">{text.slice(index, index + query.length)}</mark>,
    text.slice(index + query.length),
  ];
}
