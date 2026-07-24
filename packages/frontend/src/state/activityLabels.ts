import type { ActivityStep, NormalizedMessage } from "@openaide/app-shell-contracts";
import { firstFieldValue } from "./toolDetailsShared";

type ActivityMessage = Extract<NormalizedMessage, { kind: "activity" }>;

export type ActivityStepSemanticTitle = {
  action: "Read" | "Search";
  scope?: string;
  subjects: string[];
};

export function activitySummary(activity: ActivityMessage) {
  if (activity.steps.length > 1) return groupedActivitySummary(activity);
  const first = activity.steps[0];
  if (first?.kind === "text") {
    const kind = classifyStep(first, activity.title);
    if (kind !== "other") return countLabel(kind, 1, true) ?? humanizeToolName(activity.title);
  }
  if (first && first.kind !== "text") return countLabel(classifyStep(first, activity.title), 1, true) ?? humanizeToolName(activity.title);
  return humanizeToolName(activity.title);
}

function groupedActivitySummary(activity: ActivityMessage) {
  const counts = new Map<ActivitySummaryKind, number>();
  for (const step of activity.steps) {
    const kind = classifyStep(step, activity.title);
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  const parts = Array.from(counts, ([kind, count], index) => countLabel(kind, count, index === 0)).filter(
    (part): part is string => part !== undefined,
  );

  return parts.join(", ");
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
  const semanticTitle = activityStepSemanticTitle(step);
  if (semanticTitle) return semanticTitleText(semanticTitle);
  if (step.presentation) {
    return toolLabel(presentationAction(step.presentation.kind), presentationSubject(step) ?? "");
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
export function activityStepSemanticTitle(step: ActivityStep): ActivityStepSemanticTitle | undefined {
  if (step.kind !== "tool") return undefined;
  if (step.presentation?.kind === "read") {
    const subjects = step.presentation.subjects.map((subject) => subject.trim()).filter(Boolean);
    return subjects.length ? { action: "Read", subjects } : undefined;
  }
  if (step.name === "read") {
    const subject = (pathSubjectLabel(step) ?? step.input_summary)?.replace(/^Read\s+/i, "").trim();
    return subject ? { action: "Read", subjects: [subject] } : undefined;
  }
  if (step.name !== "search") return undefined;
  const scope = searchScopeLabel(step);
  const query = searchQueryLabel(step);
  if (query) return { action: "Search", subjects: [searchQueryPreview(query)], ...(scope ? { scope } : {}) };
  const subject = searchSubjectLabel(step);
  if (!subject) return { action: "Search", subjects: [], ...(scope ? { scope } : {}) };
  return { action: "Search", subjects: [subject], ...(scope && subject !== scope ? { scope } : {}) };
}

/** Describes the concrete action currently in flight, using the activity title when ACP normalized the tool name. */
export function activityStepProgressLabel(step: ActivityStep, activityTitle?: string) {
  if (step.kind === "thought") return "Thinking";
  if (step.kind === "command") return `Running ${step.command_label}`;
  if (step.kind === "text") return step.text;
  const collaborationLabel = collaborationProgressAction(
    `${step.name} ${activityTitle ?? ""} ${step.input_summary ?? ""}`,
  );
  if (collaborationLabel) return collaborationLabel;
  if (step.presentation) {
    return progressLabel(presentationProgressAction(step.presentation.kind), presentationSubject(step) ?? "");
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
  if (step.presentation) {
    const subject = presentationSubject(step) ?? "";
    if (step.status === "interrupted") return progressLabel("Interrupted", subject);
    if (step.status === "error") {
      return progressLabel(presentationFailureAction(step.presentation.kind), subject);
    }
    return progressLabel(presentationCompletedAction(step.presentation.kind), subject);
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
  if (step.kind !== "tool") return undefined;
  const input = step.details?.input;
  if (step.name === "search") {
    return undefined;
  }
  return input?.cwd;
}

export function activityStepStatus(step: ActivityStep) {
  if (step.kind === "text" || step.kind === "thought") return undefined;
  if (step.kind === "command" && step.exit_code !== undefined) return `exit ${step.exit_code}`;
  if (step.status === "running") return "Running";
  if (step.status === "interrupted") return "Interrupted";
  if (step.status === "error") return "Failed";
  return undefined;
}

export function activityStepPreview(step: ActivityStep) {
  if (step.kind === "text" || step.kind === "thought") return undefined;
  return step.output_preview;
}

function isCommandTool(step: Extract<ActivityStep, { kind: "tool" }>, title: string) {
  const value = stepSearchText(step, title);
  return step.name === "execute" || /\b(exec|command|shell|bash|terminal)\b/.test(value) || isCommandLine(value) || value.includes("exec_command");
}

function isTerminalInputTool(title: string) {
  return title.toLowerCase().includes("write_stdin");
}

type ActivitySummaryKind =
  | "thought"
  | "skill"
  | "read"
  | "edit"
  | "delete"
  | "move"
  | "run"
  | "search"
  | "list"
  | "fetch"
  | "thinkTool"
  | "switchMode"
  | "terminalInput"
  | "collaboration"
  | "other";

function classifyStep(step: ActivityStep, title: string): ActivitySummaryKind {
  if (step.kind === "thought") return "thought";
  if (step.kind === "command") return "run";
  if (step.kind === "text") return classifyTextStep(step.text);
  const value = stepSearchText(step, title);
  if (isTerminalInputTool(title)) return "terminalInput";
  if (step.presentation) return step.presentation.kind;
  if (step.name === "skill") return "skill";
  if (collaborationAction(value)) return "collaboration";
  if (isExecuteTool(step)) return "run";
  if (step.name === "delete") return "delete";
  if (step.name === "move") return "move";
  if (step.name === "think") return "thinkTool";
  if (step.name === "switch_mode") return "switchMode";
  if (step.name === "read" || /\bread(?:ing)?\b|\bread file\b|\bopened file\b/.test(value)) return "read";
  if (step.name === "edit" || /\b(edit|edited|update|updated|write|wrote|create|created|patch|patched)\b/.test(value)) return "edit";
  if (step.name === "search" || step.name === "web_search" || /\b(search|searched|grep|rg|find)\b/.test(value)) return "search";
  if (step.name === "fetch" || /\b(fetch|fetched|open(?:ed)? (?:page|url)|url|https?:\/\/)\b/.test(value)) return "fetch";
  if (isCommandTool(step, title)) return "run";
  return "other";
}

function classifyTextStep(text: string): ActivitySummaryKind {
  const value = text.toLowerCase();
  if (/\bread(?:ing)?\b|\bread file\b|\bopened file\b/.test(value)) return "read";
  if (/\b(edit|edits|edited|editing|update|updates|updated|updating|write|writes|wrote|writing|create|creates|created|creating|patch|patches|patched|patching)\b/.test(value)) return "edit";
  if (/\b(search|searches|searched|searching|grep|rg|find)\b/.test(value)) return "search";
  if (/\b(fetch|fetches|fetched|fetching|open(?:ed|ing)? (?:page|url)|url|https?:\/\/)\b/.test(value)) return "fetch";
  if (/\b(exec|execute|executed|executing|command|shell|bash|terminal)\b|\/bin\/(?:ba|z)?sh\b|\bnpm\b|\bgit\b/.test(value)) return "run";
  return "other";
}

function isCommandLine(value: string) {
  return /(?:^|\s)(?:git|npm|pnpm|yarn|cargo|go|node|python3?|pytest|npx|rg|grep|sed|cat|ls|curl|docker|deno|bun)\b/.test(value);
}

function stepSearchText(step: Extract<ActivityStep, { kind: "tool" }>, title: string) {
  const detailsLabel = toolSubjectLabel(step);
  return `${step.name} ${title} ${step.input_summary ?? ""} ${detailsLabel ?? ""}`.toLowerCase();
}

function countLabel(kind: ActivitySummaryKind, count: number, sentenceStart: boolean) {
  if (count === 0) return undefined;
  const labels: Record<ActivitySummaryKind, { verb?: string; single: string; plural: string }> = {
    thought: { single: "thought", plural: "thoughts" },
    skill: { verb: "activated", single: "skill", plural: "skills" },
    read: { verb: "read", single: "file", plural: "files" },
    edit: { verb: "updated", single: "file", plural: "files" },
    delete: { verb: "deleted", single: "file", plural: "files" },
    move: { verb: "moved", single: "file", plural: "files" },
    run: { verb: "ran", single: "command", plural: "commands" },
    search: { verb: "ran", single: "search", plural: "searches" },
    list: { verb: "listed", single: "directory", plural: "directories" },
    fetch: { verb: "fetched", single: "resource", plural: "resources" },
    thinkTool: { verb: "used", single: "reasoning tool", plural: "reasoning tools" },
    switchMode: { verb: "switched", single: "mode", plural: "modes" },
    terminalInput: { verb: "sent", single: "terminal input", plural: "terminal inputs" },
    collaboration: { verb: "coordinated", single: "subagent", plural: "subagents" },
    other: { verb: "called", single: "tool", plural: "tools" },
  };
  const label = labels[kind];
  if (kind === "thought") {
    const phrase = count === 1 ? "thought" : count === 2 ? "thought twice" : `thought ${count} times`;
    return sentenceStart ? capitalize(phrase) : phrase;
  }
  const noun = count === 1 ? label.single : label.plural;
  const phrase = count === 1 ? `${label.verb} ${noun}` : `${label.verb} ${count} ${noun}`;
  return sentenceStart ? capitalize(phrase) : phrase;
}

function toolSubjectLabel(step: Extract<ActivityStep, { kind: "tool" }>) {
  if (step.presentation) return presentationSubject(step);
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

function presentationSubject(step: Extract<ActivityStep, { kind: "tool" }>) {
  const subjects = step.presentation?.subjects.map((subject) => subject.trim()).filter(Boolean);
  if (!subjects?.length) return undefined;
  const joined = naturalJoin(subjects);
  if (step.presentation?.kind !== "skill") return joined;
  return subjects.length === 1 ? `${joined} skill` : `${joined} skills`;
}

function naturalJoin(values: string[]) {
  if (values.length < 2) return values[0] ?? "";
  if (values.length === 2) return `${values[0]} and ${values[1]}`;
  return `${values.slice(0, -1).join(", ")}, and ${values.at(-1)}`;
}

function semanticTitleText(title: ActivityStepSemanticTitle) {
  const subjects = naturalJoin(title.subjects);
  const actionAndSubjects = subjects ? `${title.action} ${subjects}` : title.action;
  return title.scope ? `${actionAndSubjects} in ${title.scope}` : actionAndSubjects;
}

function presentationAction(kind: NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>["kind"]) {
  if (kind === "skill") return "Activated";
  if (kind === "read") return "Read";
  if (kind === "list") return "List";
  return "Search";
}

function presentationProgressAction(kind: NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>["kind"]) {
  if (kind === "skill") return "Activating";
  if (kind === "read") return "Reading";
  if (kind === "list") return "Listing";
  return "Searching";
}

function presentationCompletedAction(kind: NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>["kind"]) {
  if (kind === "skill") return "Activated";
  if (kind === "read") return "Read";
  if (kind === "list") return "Listed";
  return "Searched";
}

function presentationFailureAction(kind: NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>["kind"]) {
  if (kind === "skill") return "Failed to activate";
  if (kind === "read") return "Failed to read";
  if (kind === "list") return "Failed to list";
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

function capitalize(value: string) {
  return value.replace(/^\w/, (letter) => letter.toUpperCase());
}
