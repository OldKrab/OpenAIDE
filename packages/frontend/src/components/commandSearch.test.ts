import { describe, expect, it } from "vitest";
import type { AgentSlashCommand } from "@openaide/app-shell-contracts";
import {
  commandTokenAtCursor,
  exactSlashCommandMatches,
  slashCommandPickerResults,
} from "./commandSearch";

describe("slash command picker search", () => {
  it("uses fuzzy command-name search with prefix and segment ranking", () => {
    expect(names(slashCommandPickerResults(commands(), "review")).slice(0, 4)).toEqual([
      "review",
      "$doomsday-review",
      "$doomsdayReview",
      "$thermo-nuclear-code-quality-review",
    ]);
    expect(names(slashCommandPickerResults(commands(), "$review")).slice(0, 3)).toEqual([
      "$doomsday-review",
      "$doomsdayReview",
      "$thermo-nuclear-code-quality-review",
    ]);
    expect(names(slashCommandPickerResults(commands(), "$day")).slice(0, 2)).toEqual([
      "$doomsday-review",
      "$doomsdayReview",
    ]);
    expect(names(slashCommandPickerResults(commands(), "$codequal"))[0]).toBe("$thermo-nuclear-code-quality-review");
    expect(names(slashCommandPickerResults(commands(), "debconf"))[0]).toBe("debug-config");
  });

  it("searches command names only, not descriptions", () => {
    expect(names(slashCommandPickerResults(commands(), "strict"))).toEqual([]);
  });

  it("returns no command UI when no catalog exists", () => {
    expect(slashCommandPickerResults(undefined, "$review")).toEqual([]);
  });

  it("returns the command list for an empty slash query", () => {
    expect(names(slashCommandPickerResults(commands(), "")).slice(0, 3)).toEqual([
      "review",
      "$doomsday-review",
      "$doomsdayReview",
    ]);
  });
});

describe("slash command tokenization", () => {
  it("detects whitespace-bounded tokens at the cursor", () => {
    expect(commandTokenAtCursor("/", 1)).toEqual({ start: 0, end: 1, name: "" });
    expect(commandTokenAtCursor("run /$review now", 7)).toEqual({ start: 4, end: 12, name: "$review" });
    expect(commandTokenAtCursor("/compact", 8)).toEqual({ start: 0, end: 8, name: "compact" });
    expect(commandTokenAtCursor("path/$review", 7)).toBeUndefined();
    expect(commandTokenAtCursor("run /$review:", 7)).toBeUndefined();
  });

  it("decorates only exact current-catalog commands", () => {
    const matches = exactSlashCommandMatches(
      "Use /$doomsday-review and /review but not /missing.",
      commands(),
    );

    expect(matches.map((match) => match.command.name)).toEqual(["$doomsday-review", "review"]);
    expect(matches.map((match) => match.token.name)).toEqual(["$doomsday-review", "review"]);
  });
});

function commands(): AgentSlashCommand[] {
  return [
    command("review", "Review changes."),
    command("$doomsday-review", "Strict PR/branch review."),
    command("$doomsdayReview", "Strict PR/branch review."),
    command("$thermo-nuclear-code-quality-review", "Maintainability review."),
    command("$codebase-design", "Design modules."),
    command("debug-config", "Show config."),
  ];
}

function command(name: string, description: string): AgentSlashCommand {
  return { name, description };
}

function names(commands: AgentSlashCommand[]) {
  return commands.map((command) => command.name);
}
