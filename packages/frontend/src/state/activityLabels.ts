import type { ActivityStep, NormalizedMessage } from "@openaide/app-shell-contracts";
import {
  presentationKind,
  presentationSemanticTitle,
  presentationSubject,
  semanticTitleText,
  type ActivityStepSemanticAction,
  type ActivityStepSemanticTitle,
  type PresentationDisplayKind,
} from "./activityPresentationLabels";
import { firstFieldValue } from "./toolDetailsShared";

type ActivityMessage = Extract<NormalizedMessage, { kind: "activity" }>;

export type { ActivityStepSemanticTitle } from "./activityPresentationLabels";

export type ActivityToolKind =
  | "skill"
  | "read"
  | "list"
  | "edit"
  | "delete"
  | "move"
  | "search"
  | "inspect"
  | "web_search"
  | "execute"
  | "think"
  | "fetch"
  | "switch_mode"
  | "terminal_input"
  | "collaboration"
  | "other";

export function activitySummary(activity: ActivityMessage) {
  if (activity.steps.length > 1) return groupedActivitySummary(activity);
  const first = activity.steps[0];
  if (first?.kind === "text") {
    const kind = classifyStep(first);
    if (kind !== "other") return countLabel(kind, 1, true) ?? humanizeToolName(activity.title);
  }
  if (first && first.kind !== "text") {
    return summarizeKinds(classifyStepKinds(first, activity.title)) ?? humanizeToolName(activity.title);
  }
  return humanizeToolName(activity.title);
}

function groupedActivitySummary(activity: ActivityMessage) {
  return summarizeKinds(activity.steps.flatMap((step) => classifyStepKinds(step))) ?? humanizeToolName(activity.title);
}

function summarizeKinds(kinds: ActivitySummaryKind[]) {
  const counts = new Map<ActivitySummaryKind, number>();
  for (const kind of kinds) {
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  const parts = Array.from(counts, ([kind, count], index) => countLabel(kind, count, index === 0)).filter(
    (part): part is string => part !== undefined,
  );

  return parts.length ? parts.join(", ") : undefined;
}

export function activityStatusLabel(status: ActivityMessage["status"]) {
  if (status === "running") return "Running";
  if (status === "interrupted") return "Interrupted";
  return undefined;
}

export function activityStepLabel(step: ActivityStep) {
  if (step.kind === "thought") return "Thought";
  if (step.kind === "command") return step.command_label;
  if (step.kind === "text") return step.text;
  if (step.kind === "subagent") return step.title ?? subagentName(step.name);
  if (step.name === "collaboration") return "Wait for subagents";
  const semanticTitle = activityStepSemanticTitle(step);
  if (semanticTitle) return semanticTitleText(semanticTitle);
  if (step.presentation) {
    return toolLabel(
      presentationAction(presentationKind(step.presentation)),
      presentationSubject(step.presentation) ?? "",
    );
  }
  const subject = toolSubjectLabel(step);
  if (isExecuteTool(step)) return subject ?? humanizeToolName(step.name);
  if (step.name === "think") return "Reasoning tool";
  if (step.name === "switch_mode") return subject ? `Switch mode to ${subject}` : "Switch mode";
  if (step.name === "web_search" && subject) return `Web search: ${subject}`;
  const action = toolActionLabel(step.name);
  if (subject && action) return toolLabel(action, subject);
  if (subject) return subject;
  return action ?? humanizeToolName(step.name);
}

/** Structured compact-title roles let the UI add hierarchy without inventing tool-specific colors. */
export function activityStepSemanticTitle(
  step: ActivityStep,
): ActivityStepSemanticTitle | undefined {
  if (step.kind !== "tool") return undefined;
  const presentation = step.presentation;
  if (presentation) return presentationSemanticTitle(presentation);
  if (step.name === "read") {
    const subject = (pathSubjectLabel(step) ?? step.input_summary)?.replace(/^Read\s+/i, "").trim();
    return subject ? semanticTitle([{ action: "Read", subjects: [subject] }]) : undefined;
  }
  if (step.name !== "search") return undefined;
  const scope = searchScopeLabel(step);
  const query = searchQueryLabel(step);
  if (query) {
    return semanticTitle([{
        action: "Search",
        subjects: [searchQueryPreview(query)],
        ...(scope ? { scope } : {}),
      }]);
  }
  const subject = searchSubjectLabel(step);
  if (!subject) {
    return semanticTitle([{ action: "Search", subjects: [], ...(scope ? { scope } : {}) }]);
  }
  return semanticTitle([{
      action: "Search",
      subjects: [subject],
      ...(scope && subject !== scope ? { scope } : {}),
    }]);
}

function semanticTitle(actions: ActivityStepSemanticAction[]): ActivityStepSemanticTitle {
  const title = { actions, tooltip: "" };
  return { actions, tooltip: semanticTitleText(title) };
}

/** Describes the concrete action currently in flight, using the activity title when ACP normalized the tool name. */
export function activityStepProgressLabel(step: ActivityStep, activityTitle?: string) {
  if (step.kind === "thought") return "Thinking";
  if (step.kind === "command") return `Running ${step.command_label}`;
  if (step.kind === "text") return step.text;
  if (step.kind === "subagent") {
    if (step.title) return step.title;
    return subagentEventProgressLabel(step.events.at(-1), subagentName(step.name));
  }
  if (step.name === "collaboration") return "Waiting for subagents";
  const collaborationLabel = collaborationProgressAction(
    `${step.name} ${activityTitle ?? ""} ${step.input_summary ?? ""}`,
  );
  if (collaborationLabel) return collaborationLabel;
  if (step.presentation) {
    return progressLabel(
      presentationProgressAction(presentationKind(step.presentation)),
      presentationSubject(step.presentation) ?? "",
    );
  }
  const subject = toolSubjectLabel(step);
  if (isExecuteTool(step)) return progressLabel("Running", subject ?? humanizeToolName(step.name));
  if (step.name === "think") return "Using reasoning tool";
  if (step.name === "web_search") return progressLabel(subject ? "Searching the web for" : "Searching the web", subject ?? "");
  const actions: Record<string, string> = {
    skill: "Activating",
    read: "Reading",
    edit: "Updating",
    delete: "Deleting",
    move: "Moving",
    search: "Searching",
    fetch: "Opening",
    think: "Using reasoning tool",
    switch_mode: "Switching mode to",
  };
  return progressLabel(actions[step.name] ?? "Using", subject ?? humanizeToolName(step.name));
}

/** Describes the newest finished action for the live footer without repeating its activity-group title. */
export function activityStepCompletedLabel(step: ActivityStep) {
  if (step.kind === "thought") return "Thought";
  if (step.kind === "command") {
    if (step.status === "interrupted") return `Command interrupted: ${step.command_label}`;
    return step.status === "error" ? `Command failed: ${step.command_label}` : `Ran ${step.command_label}`;
  }
  if (step.kind === "text") return step.text;
  if (step.kind === "subagent") {
    if (step.title) return step.title;
    const name = subagentName(step.name);
    return subagentEventCompletedLabel(step.events.at(-1), name);
  }
  if (step.name === "collaboration") {
    if (step.status === "error") return "Failed while waiting for subagents";
    if (step.status === "interrupted") return "Stopped waiting for subagents";
    return "Waited for subagents";
  }
  if (step.presentation) {
    const subject = presentationSubject(step.presentation) ?? "";
    if (step.status === "interrupted") return progressLabel("Interrupted", subject);
    if (step.status === "error") {
      return progressLabel(presentationFailureAction(presentationKind(step.presentation)), subject);
    }
    return progressLabel(presentationCompletedAction(presentationKind(step.presentation)), subject);
  }
  const subject = toolSubjectLabel(step);
  if (step.status === "interrupted") return progressLabel("Interrupted", subject ?? humanizeToolName(step.name));
  if (step.status === "error") return progressLabel("Failed to use", subject ?? humanizeToolName(step.name));
  if (isExecuteTool(step)) return progressLabel("Ran", subject ?? "command");
  if (step.name === "think") return "Used reasoning tool";
  if (step.name === "web_search") return progressLabel(subject ? "Searched the web for" : "Searched the web", subject ?? "");
  const actions: Record<string, string> = {
    skill: "Activated",
    read: "Read",
    edit: "Updated",
    delete: "Deleted",
    move: "Moved",
    search: "Searched",
    fetch: "Opened",
    think: "Used reasoning tool",
    switch_mode: "Switched mode to",
  };
  return progressLabel(actions[step.name] ?? "Used", subject ?? humanizeToolName(step.name));
}

function collaborationProgressAction(value: string) {
  const normalized = value.toLowerCase();
  if (/\bwait_agent\b/.test(normalized)) return "Waiting for subagent";
  if (/\bwait\b/.test(normalized) && /\b(?:senderthreadid|receiverthreadids|agentsstates)\b/.test(normalized)) {
    return "Waiting for subagent";
  }
  return undefined;
}

export function activityStepContext(step: ActivityStep) {
  if (step.kind === "subagent") {
    if (step.title) return undefined;
    const parents = step.path.slice(0, -1).map(subagentName).filter(Boolean);
    return parents.length ? parents.join(" › ") : undefined;
  }
  if (step.kind !== "tool") return undefined;
  // Structured presentation already owns user-facing scope; raw cwd would
  // duplicate it and steal width from the semantic title.
  if (step.presentation || step.name === "search") {
    return undefined;
  }
  return step.details?.input?.cwd;
}

export function activityStepStatus(step: ActivityStep) {
  if (step.kind === "text" || step.kind === "thought") return undefined;
  if (step.kind === "command" && step.exit_code !== undefined) return `exit ${step.exit_code}`;
  if (step.status === "running") return "Running";
  if (step.status === "interrupted") return "Interrupted";
  if (step.status === "error") return "Failed";
  if (
    step.status === "completed"
    && ((step.kind === "subagent" && step.title) || (step.kind === "tool" && step.name === "collaboration"))
  ) {
    return "Completed";
  }
  return undefined;
}

export function activityStepPreview(step: ActivityStep) {
  if (step.kind === "text" || step.kind === "thought" || step.kind === "subagent") return undefined;
  return step.output_preview;
}

type ActivitySummaryKind =
  | "thought"
  | "read"
  | "edit"
  | "delete"
  | "run"
  | "search"
  | "subagentInteraction"
  | "other";

function classifyStep(step: ActivityStep, legacyToolName?: string): ActivitySummaryKind {
  if (step.kind === "thought") return "thought";
  if (step.kind === "command") return "run";
  if (step.kind === "text") return classifyTextStep(step.text);
  if (step.kind === "subagent") return "subagentInteraction";
  return summaryKindForTool(activityToolKind(step, legacyToolName));
}

/** Expands composite Tool presentation into the real actions shown in the group summary. */
function classifyStepKinds(step: ActivityStep, legacyToolName?: string): ActivitySummaryKind[] {
  if (step.kind !== "tool" || !step.presentation || presentationKind(step.presentation) !== "inspect") {
    return [classifyStep(step, legacyToolName)];
  }
  const kinds = step.presentation.actions.map((action) =>
    summaryKindForTool(action.kind === "view" ? "read" : action.kind)
  );
  return kinds.length ? kinds : ["other"];
}

/**
 * Resolves one Tool meaning for every visual consumer. Trusted presentation
 * and explicit names win; legacy heuristics inspect only the Tool itself.
 */
export function activityToolKind(
  step: Extract<ActivityStep, { kind: "tool" }>,
  legacyToolName?: string,
): ActivityToolKind {
  if (step.presentation) {
    const kind = presentationKind(step.presentation);
    return kind === "view" ? "read" : kind;
  }
  const namedKinds: Record<string, ActivityToolKind> = {
    skill: "skill",
    read: "read",
    list: "list",
    edit: "edit",
    delete: "delete",
    move: "move",
    search: "search",
    web_search: "web_search",
    execute: "execute",
    exec_command: "execute",
    think: "think",
    fetch: "fetch",
    switch_mode: "switch_mode",
    write_stdin: "terminal_input",
    collaboration: "collaboration",
  };
  const namedKind = namedKinds[step.name] ?? (legacyToolName ? namedKinds[legacyToolName] : undefined);
  if (namedKind) return namedKind;

  const value = stepSearchText(step);
  if (collaborationAction(value)) return "collaboration";
  if (/\bread(?:ing)?\b|\bread file\b|\bopened file\b/.test(value)) return "read";
  if (/\b(edit|edited|update|updated|write|wrote|create|created|patch|patched)\b/.test(value)) return "edit";
  if (/\b(search|searched|grep|rg|find)\b/.test(value)) return "search";
  if (/\b(fetch|fetched|open(?:ed)? (?:page|url)|url|https?:\/\/)\b/.test(value)) return "fetch";
  if (/\b(exec|command|shell|bash|terminal)\b/.test(value) || isCommandLine(value) || value.includes("exec_command")) {
    return "execute";
  }
  return "other";
}

function summaryKindForTool(kind: ActivityToolKind): ActivitySummaryKind {
  if (kind === "collaboration") return "subagentInteraction";
  if (kind === "skill") return "other";
  if (kind === "web_search") return "search";
  if (kind === "move") return "edit";
  if (kind === "execute") return "run";
  if (kind === "think") return "thought";
  if (kind === "fetch") return "search";
  if (kind === "list") return "search";
  if (kind === "switch_mode") return "other";
  if (kind === "terminal_input") return "other";
  if (kind === "inspect") return "other";
  return kind;
}

function classifyTextStep(text: string): ActivitySummaryKind {
  const value = text.toLowerCase();
  if (/\bread(?:ing)?\b|\bread file\b|\bopened file\b/.test(value)) return "read";
  if (/\b(edit|edits|edited|editing|update|updates|updated|updating|write|writes|wrote|writing|create|creates|created|creating|patch|patches|patched|patching)\b/.test(value)) return "edit";
  if (/\b(search|searches|searched|searching|grep|rg|find)\b/.test(value)) return "search";
  if (/\b(fetch|fetches|fetched|fetching|open(?:ed|ing)? (?:page|url)|url|https?:\/\/)\b/.test(value)) return "search";
  if (/\b(exec|execute|executed|executing|command|shell|bash|terminal)\b|\/bin\/(?:ba|z)?sh\b|\bnpm\b|\bgit\b/.test(value)) return "run";
  return "other";
}

function isCommandLine(value: string) {
  return /(?:^|\s)(?:git|npm|pnpm|yarn|cargo|go|node|python3?|pytest|npx|rg|grep|sed|cat|ls|curl|docker|deno|bun)\b/.test(value);
}

function stepSearchText(step: Extract<ActivityStep, { kind: "tool" }>) {
  const detailsLabel = toolSubjectLabel(step);
  return `${step.name} ${step.input_summary ?? ""} ${detailsLabel ?? ""}`.toLowerCase();
}

function countLabel(kind: ActivitySummaryKind, count: number, sentenceStart: boolean) {
  if (count === 0) return undefined;
  const labels: Record<ActivitySummaryKind, { verb?: string; single: string; plural: string }> = {
    thought: { single: "thought", plural: "thoughts" },
    read: { verb: "read", single: "file", plural: "files" },
    edit: { verb: "updated", single: "file", plural: "files" },
    delete: { verb: "deleted", single: "file", plural: "files" },
    run: { verb: "ran", single: "command", plural: "commands" },
    search: { verb: "ran", single: "search", plural: "searches" },
    subagentInteraction: { single: "subagent interaction", plural: "subagent interactions" },
    other: { verb: "called", single: "tool", plural: "tools" },
  };
  const label = labels[kind];
  if (kind === "subagentInteraction") {
    const phrase = count === 1 ? "interacted with subagent" : `${count} ${label.plural}`;
    return sentenceStart ? capitalize(phrase) : phrase;
  }
  if (kind === "thought") {
    const phrase = count === 1 ? "thought" : count === 2 ? "thought twice" : `thought ${count} times`;
    return sentenceStart ? capitalize(phrase) : phrase;
  }
  const noun = count === 1 ? label.single : label.plural;
  const phrase = count === 1 ? `${label.verb} ${noun}` : `${label.verb} ${count} ${noun}`;
  return sentenceStart ? capitalize(phrase) : phrase;
}

function toolSubjectLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  if (step.presentation) return presentationSubject(step.presentation);
  if (step.name === "skill") return skillSubjectLabel(step.input_summary);
  const collaborationLabel = collaborationAction(`${step.name} ${step.input_summary ?? ""}`);
  if (collaborationLabel) return collaborationLabel;
  if (step.name === "search" || step.name === "web_search") return searchSubjectLabel(step);
  if (step.name === "read" || step.name === "edit" || step.name === "delete" || step.name === "move") {
    return pathSubjectLabel(step) ?? step.input_summary;
  }
  if (step.name === "fetch") return fetchSubjectLabel(step) ?? step.input_summary;
  const detailsLabel = toolDetailsLabel(step);
  if (detailsLabel && (!step.input_summary || isContextOnlySummary(step, step.input_summary))) return detailsLabel;
  return step.input_summary ?? detailsLabel;
}

function presentationAction(kind: PresentationDisplayKind) {
  if (kind === "skill") return "Activated";
  if (kind === "read") return "Read";
  if (kind === "view") return "View";
  if (kind === "list") return "List";
  if (kind === "inspect") return "Inspect";
  return "Search";
}

function presentationProgressAction(kind: PresentationDisplayKind) {
  if (kind === "skill") return "Activating";
  if (kind === "read") return "Reading";
  if (kind === "view") return "Viewing";
  if (kind === "list") return "Listing";
  if (kind === "inspect") return "Inspecting";
  return "Searching";
}

function presentationCompletedAction(kind: PresentationDisplayKind) {
  if (kind === "skill") return "Activated";
  if (kind === "read") return "Read";
  if (kind === "view") return "Viewed";
  if (kind === "list") return "Listed";
  if (kind === "inspect") return "Inspected";
  return "Searched";
}

function presentationFailureAction(kind: PresentationDisplayKind) {
  if (kind === "skill") return "Failed to activate";
  if (kind === "read") return "Failed to read";
  if (kind === "view") return "Failed to view";
  if (kind === "list") return "Failed to list";
  if (kind === "inspect") return "Failed to inspect";
  return "Failed to search";
}

function skillSubjectLabel(value: string | undefined) {
  const name = value?.trim();
  if (!name || name.toLowerCase().endsWith(" skill")) return name;
  return `${name} skill`;
}

function collaborationAction(value: string) {
  const normalized = value.toLowerCase();
  if (/\bspawn_agent\b/.test(normalized)) return "Started subagent";
  if (/\b(?:followup_task|send_message)\b/.test(normalized)) return "Messaged subagent";
  if (/\bwait_agent\b/.test(normalized)) return "Waited for subagent";
  if (/\blist_agents\b/.test(normalized)) return "Checked subagents";
  if (/\binterrupt_agent\b/.test(normalized)) return "Stopped subagent";
  return undefined;
}

function searchSubjectLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const input = step.details?.input;
  if (input?.query) return input.query;
  const queryField = ["query", "q", "pattern"].map((name) => firstFieldValue(input?.fields, name)).find(Boolean);
  if (queryField) return queryField;
  const command = commandLabel(input?.command);
  if (command && step.input_summary && isContextOnlySummary(step, step.input_summary)) return command;
  return searchTitleParts(step.input_summary)?.query ?? step.input_summary ?? command;
}

function searchQueryLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const input = step.details?.input;
  return (
    input?.query ??
    ["query", "q", "pattern"].map((name) => firstFieldValue(input?.fields, name)).find(Boolean) ??
    searchTitleParts(step.input_summary)?.query
  );
}

function searchScopeLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const input = step.details?.input;
  const fieldScope = ["path", "file", "cwd"].map((name) => firstFieldValue(input?.fields, name)).find(Boolean);
  return input?.path ?? fieldScope ?? searchTitleParts(step.input_summary)?.scope ?? input?.cwd;
}

function searchTitleParts(value: string | undefined) {
  const title = value?.trim();
  if (!title) return undefined;
  const quoted = /^Search for (['"`])([\s\S]*)\1 in (.+)$/i.exec(title);
  if (quoted) return { query: quoted[2], scope: quoted[3].trim() };
  const plain = /^Search(?: for)? (.+) in (.+)$/i.exec(title);
  if (plain) return { query: plain[1].trim(), scope: plain[2].trim() };
  return undefined;
}

function searchQueryPreview(query: string) {
  const normalized = query.replace(/\s+/g, " ").trim();
  const characters = Array.from(normalized);
  const preview = characters.length > 48 ? `${characters.slice(0, 47).join("")}…` : normalized;
  return looksLikeRegex(normalized) ? `/${preview}/` : `“${preview}”`;
}

function looksLikeRegex(query: string) {
  return (
    /\\(?:[\\^$.*+?(){}|]|\[|\])/.test(query) ||
    /\[[^\]]+\]/.test(query) ||
    /\((?:\?:)?[^)]*\)/.test(query) ||
    /\{\d+(?:,\d*)?\}/.test(query) ||
    /(^|[^\\])\|/.test(query) ||
    /(^|[^\\])\.[*+?]/.test(query) ||
    query.startsWith("^") ||
    (query.endsWith("$") && !query.endsWith("\\$"))
  );
}

function pathSubjectLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const details = step.details;
  const path = details?.locations?.[0]?.path ?? diffPath(details?.content) ?? details?.input?.path;
  return path ? pathLeaf(path) : undefined;
}

function diffPath(content: NonNullable<Extract<ActivityStep, { kind: "tool" }>["details"]>["content"] | undefined) {
  return content?.find((item) => item.kind === "diff")?.path;
}

function fetchSubjectLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const url = step.details?.input?.url;
  return url ? compactUrl(url) : undefined;
}

function toolDetailsLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  const input = step.details?.input;
  if (!input) return undefined;
  const command = commandLabel(input.command);
  if (command) return command;
  if (input.path) return pathLeaf(input.path);
  if (input.query) return input.query;
  if (input.url) return input.url;
  return undefined;
}

function toolActionLabel(name: string) {
  const labels: Record<string, string> = {
    skill: "Activated",
    read: "Read",
    edit: "Edit",
    delete: "Delete",
    move: "Move",
    search: "Search",
    web_search: "Web search",
    fetch: "Fetch",
    switch_mode: "Switch mode",
    think: "Reasoning tool",
    collaboration: "Wait for subagents",
  };
  return labels[name];
}

function isExecuteTool(step: Extract<ActivityStep, { kind: "tool" }>) {
  return step.name === "execute" || step.name === "exec_command";
}

function progressLabel(action: string, subject: string) {
  return subject ? `${action} ${subject}` : action;
}

function toolLabel(action: string, subject: string) {
  const normalizedSubject = subject.trim();
  if (!normalizedSubject) return action;
  if (normalizedSubject.toLowerCase().startsWith(`${action.toLowerCase()} `)) return normalizedSubject;
  return `${action} ${normalizedSubject}`;
}

function isContextOnlySummary(step: Extract<ActivityStep, { kind: "tool" }>, summary: string) {
  const input = step.details?.input;
  if (!input) return false;
  return summary === input.cwd && Boolean(commandLabel(input.command) || input.path || input.query || input.url);
}

function commandLabel(command: string[] | undefined) {
  const parts = command?.map((part) => part.trim()).filter(Boolean) ?? [];
  if (parts.length === 0) return undefined;
  if (parts.length >= 3 && isShellLauncher(parts[0]) && parts[1] === "-lc") return parts.slice(2).join(" ");
  return parts.join(" ");
}

function isShellLauncher(value: string) {
  return ["sh", "bash", "zsh"].includes(pathLeaf(value).toLowerCase());
}

function pathLeaf(value: string) {
  return value
    .trim()
    .replace(/^['"`]|['"`]$/g, "")
    .replace(/[\\/]+$/g, "")
    .split(/[\\/]/)
    .filter(Boolean)
    .at(-1) ?? value;
}

function compactUrl(value: string) {
  try {
    const url = new URL(value);
    return `${url.hostname}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return value;
  }
}

function humanizeToolName(value: string) {
  const compact = value.trim();
  if (!compact || compact === "other") return "Tool";
  if (compact === "execute" || compact === "exec_command") return "command";
  if (compact === "write_stdin") return "terminal input";
  return compact
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^\w/, (letter) => letter.toUpperCase());
}

function subagentName(value: string) {
  return value.trim().replace(/[_-]+/g, " ").replace(/\s+/g, " ");
}

function subagentEventProgressLabel(
  event: Extract<ActivityStep, { kind: "subagent" }>["events"][number] | undefined,
  name: string,
) {
  if (event === "interacted") return `Checking in with ${name}`;
  if (event === "completed") return `${name} completed`;
  if (event === "failed") return `${name} failed`;
  if (event === "stopped") return `${name} stopped`;
  if (event === "running") return `${name} is working`;
  return `Delegating to ${name}`;
}

function subagentEventCompletedLabel(
  event: Extract<ActivityStep, { kind: "subagent" }>["events"][number] | undefined,
  name: string,
) {
  if (event === "interacted") return `Checked in with ${name}`;
  if (event === "completed") return `${name} completed`;
  if (event === "failed") return `${name} failed`;
  if (event === "stopped") return `${name} stopped`;
  if (event === "running") return `${name} is working`;
  return `Delegated to ${name}`;
}

function capitalize(value: string) {
  return value.replace(/^\w/, (letter) => letter.toUpperCase());
}
