import type { ReactNode } from "react";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { exactSlashCommandMatches, slashCommandDisplayName } from "./commandSearch";
import { fileMentionRanges } from "./ComposerFileMentions";

type SlashCommandTextProps = {
  text: string;
  commands?: AgentCommandsCatalog;
};

export function SlashCommandText({ text, commands }: SlashCommandTextProps) {
  const commandMatches = exactSlashCommandMatches(text, commands?.commands).map((match) => ({
    end: match.token.end,
    node: (
      <span
        className="slash-command-token reference-token"
        key={`command-${match.token.start}-${match.token.end}`}
        title={`${slashCommandDisplayName(match.command)}${match.command.input_hint ? ` ${match.command.input_hint}` : ""}: ${match.command.description}`}
      >
        <ReferenceToken value={text.slice(match.token.start, match.token.end)} />
      </span>
    ),
    start: match.token.start,
  }));
  const fileMatches = fileMentionRanges(text).map((range) => ({
    end: range.end,
    node: (
      <span className="file-mention-token reference-token" key={`file-${range.start}-${range.end}`}>
        <ReferenceToken value={text.slice(range.start, range.end)} />
      </span>
    ),
    start: range.start,
  }));
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

function ReferenceToken({ value }: { value: string }) {
  return (
    <>
      <span className="reference-token-sigil">{value.slice(0, 1)}</span>
      <span className="reference-token-label">{value.slice(1)}</span>
    </>
  );
}
