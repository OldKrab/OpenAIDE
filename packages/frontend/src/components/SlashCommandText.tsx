import type { ReactNode } from "react";
import type { AgentCommandsCatalog } from "@openaide/app-shell-contracts";
import { exactSlashCommandMatches, slashCommandDisplayName } from "./commandSearch";

type SlashCommandTextProps = {
  text: string;
  commands?: AgentCommandsCatalog;
};

export function SlashCommandText({ text, commands }: SlashCommandTextProps) {
  const matches = exactSlashCommandMatches(text, commands?.commands);
  if (!matches.length) return <>{text}</>;

  const nodes: ReactNode[] = [];
  let cursor = 0;
  for (const match of matches) {
    if (match.token.start > cursor) nodes.push(text.slice(cursor, match.token.start));
    const hint = match.command.input_hint ? ` ${match.command.input_hint}` : "";
    nodes.push(
      <span
        className="slash-command-token"
        key={`${match.token.start}-${match.token.end}`}
        title={`${slashCommandDisplayName(match.command)}${hint}: ${match.command.description}`}
      >
        {text.slice(match.token.start, match.token.end)}
      </span>,
    );
    cursor = match.token.end;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return <>{nodes}</>;
}
