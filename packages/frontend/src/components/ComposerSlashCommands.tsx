import type { AgentCommandsCatalog, AgentSlashCommand } from "@openaide/app-shell-contracts";
import { useId } from "react";
import {
  commandTokenAtCursor,
  slashCommandDisplayName,
  slashCommandPickerResults,
  type CommandToken,
} from "./commandSearch";
import { EditorListbox } from "./Popup";

type SlashCommandPickerProps = {
  activeIndex: number;
  commands: AgentSlashCommand[];
  id?: string;
  onSelect: (command: AgentSlashCommand) => void;
};

export type SlashPickerState = {
  activeIndex: number;
  results: AgentSlashCommand[];
  token: CommandToken;
};

export function SlashCommandPicker({ activeIndex, commands, id, onSelect }: SlashCommandPickerProps) {
  const activeCommand = commands[activeIndex] ?? commands[0];
  const generatedId = useId();
  const listboxId = id ?? `slash-commands-${generatedId}`;
  return (
    <EditorListbox className="composer-slash-popover" id={listboxId} label="Slash commands">
      <div className="composer-slash-results">
        {commands.map((command, index) => (
          <button
            aria-selected={index === activeIndex}
            className="composer-slash-option"
            id={`${listboxId}-option-${index}`}
            key={command.name}
            onClick={() => onSelect(command)}
            onMouseDown={(event) => event.preventDefault()}
            role="option"
            title={command.description}
            type="button"
          >
            <span className="composer-slash-name">{slashCommandDisplayName(command)}</span>
            <span className="composer-slash-description">{command.description}</span>
          </button>
        ))}
      </div>
      {activeCommand ? (
        <aside className="composer-slash-doc" aria-label="Command details">
          <strong>{slashCommandDisplayName(activeCommand)}</strong>
          <span>{activeCommand.description}</span>
          {activeCommand.input_hint ? <small>Argument: {activeCommand.input_hint}</small> : null}
        </aside>
      ) : null}
    </EditorListbox>
  );
}

export function replaceCommandToken(text: string, token: CommandToken, command: AgentSlashCommand) {
  const commandText = slashCommandDisplayName(command);
  const tail = text.slice(token.end);
  const needsSpace = tail.length === 0;
  const next = `${text.slice(0, token.start)}${commandText}${needsSpace ? " " : ""}${tail}`;
  return {
    cursor: token.start + commandText.length + (needsSpace ? 1 : 0),
    text: next,
  };
}

export function commandCatalogKey(catalog: AgentCommandsCatalog | undefined) {
  if (!catalog) return "unavailable";
  return `${catalog.status}:${catalog.commands.map((command) =>
    `${command.name}\u0000${command.description}\u0000${command.input_hint ?? ""}`).join("\u0001")}`;
}

export function slashPickerForCatalog(
  catalog: AgentCommandsCatalog | undefined,
  value: string,
  cursor: number,
): SlashPickerState | undefined {
  const token = catalog?.status === "ready" ? commandTokenAtCursor(value, cursor) : undefined;
  if (!catalog || !token) return undefined;
  const results = slashCommandPickerResults(catalog.commands, token.name);
  return results.length ? { activeIndex: 0, results, token } : undefined;
}
