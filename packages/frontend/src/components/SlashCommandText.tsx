import type { ReactNode } from "react";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { exactSlashCommandMatches } from "./commandSearch";
import {
  fileMentionPath,
  fileMentionRanges,
  fileReferenceDetails,
} from "./ComposerFileMentions";

type SlashCommandTextProps = {
  text: string;
  commands?: AgentCommandsCatalog;
};

export function SlashCommandText({ text, commands }: SlashCommandTextProps) {
  const commandMatches = exactSlashCommandMatches(text, commands?.commands).map((match) => {
    const label = text.slice(match.token.start, match.token.end);
    return {
      end: match.token.end,
      node: (
        <span
          className="slash-command-token reference-token"
          data-reference-description={match.command.description}
          data-reference-kind="command"
          data-reference-label={label}
          data-reference-type="Skill"
          key={`command-${match.token.start}-${match.token.end}`}
        >
          {label}
        </span>
      ),
      start: match.token.start,
    };
  });
  const fileMatches = fileMentionRanges(text).map((range) => {
    const mention = text.slice(range.start, range.end);
    const file = fileReferenceDetails(fileMentionPath(mention));
    return {
      end: range.end,
      node: (
        <span
          className="file-mention-token reference-token"
          data-reference-description={`${file.type} · ${file.location}`}
          data-reference-file-kind={file.kind}
          data-reference-kind="file"
          data-reference-label={file.name}
          data-reference-path={file.path}
          data-reference-type="Workspace file"
          key={`file-${range.start}-${range.end}`}
        >
          {mention}
        </span>
      ),
      start: range.start,
    };
  });
  const matches = [...commandMatches, ...fileMatches].sort((left, right) => left.start - right.start);
  if (!matches.length) return <>{text}</>;

  const nodes: ReactNode[] = [];
  let cursor = 0;
  for (const match of matches) {
    if (match.start < cursor) continue;
    if (match.start > cursor) nodes.push(text.slice(cursor, match.start));
    nodes.push(match.node);
    cursor = match.end;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return <>{nodes}</>;
}
