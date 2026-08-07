import type { ActivityStep } from "@openaide/app-shell-contracts";

type ToolPresentation = NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>;
type ToolPresentationAction = ToolPresentation["actions"][number];

export type ActivityStepSemanticTitle = {
  actions: ActivityStepSemanticAction[];
  /** Full, safe semantic title used when compact row text is elided. */
  tooltip: string;
};

export type ActivityStepSemanticAction = {
  action: "Activated" | "List" | "Read" | "Search" | "Search file names for" | "View";
  scope?: string;
  subjects: string[];
};

export type PresentationDisplayKind =
  ToolPresentation["actions"][number]["kind"] | "inspect";

export function presentationSemanticTitle(
  presentation: ToolPresentation,
): ActivityStepSemanticTitle | undefined {
  const fullActions = semanticPresentationActions(
    presentation.actions,
    presentation.actions.length,
    false,
  );
  const compactActions = semanticPresentationActions(
    compactPresentationActions(presentation.actions),
    presentation.actions.length,
    true,
  );
  if (!fullActions || !compactActions) return undefined;
  return {
    actions: compactActions,
    tooltip: semanticTitleText({ actions: fullActions, tooltip: "" }),
  };
}

function semanticPresentationActions(
  actions: ToolPresentationAction[],
  presentationActionCount: number,
  compact: boolean,
) {
  const semanticActions: ActivityStepSemanticAction[] = [];
  for (const action of actions) {
    const semanticAction = semanticPresentationAction(action, presentationActionCount, compact);
    if (!semanticAction) return undefined;
    semanticActions.push(semanticAction);
  }
  return semanticActions;
}

function semanticPresentationAction(
  action: ToolPresentationAction,
  presentationActionCount: number,
  compact: boolean,
): ActivityStepSemanticAction | undefined {
  if (action.kind === "search") {
    const query = action.query.trim();
    if (!query) return undefined;
    const scopes = action.scopes.map(displayWorkspace).filter(Boolean);
    const visibleScopes = compact ? shortestUniqueSuffixes(scopes) : scopes;
    return {
      action: action.target === "paths" ? "Search file names for" : "Search",
      subjects: [`“${query}”`],
      ...(visibleScopes.length ? { scope: naturalJoin(visibleScopes) } : {}),
    };
  }

  const mixedSkill = action.kind === "skill" && presentationActionCount > 1;
  if (!mixedSkill && action.kind !== "read" && action.kind !== "view") return undefined;
  const subjects = action.subjects.map((subject) => subject.trim()).filter(Boolean);
  if (!subjects.length) return undefined;
  const visibleSubjects = compact ? summarizeSubjects(subjects) : subjects;
  return {
    action: mixedSkill ? "Activated" : action.kind === "view" ? "View" : "Read",
    subjects: mixedSkill
      ? appendSkillNoun(visibleSubjects, subjects.length)
      : visibleSubjects,
  };
}

/** Compact chrome groups repeated file operations; the tooltip retains protocol order. */
function compactPresentationActions(actions: ToolPresentationAction[]) {
  const compacted: ToolPresentationAction[] = [];
  for (const action of actions) {
    if (action.kind !== "read" && action.kind !== "view") {
      compacted.push(action);
      continue;
    }
    const existing = compacted.find((candidate) => candidate.kind === action.kind);
    if (existing?.kind === action.kind && "subjects" in existing) {
      existing.subjects = orderedUnique([...existing.subjects, ...action.subjects]);
      continue;
    }
    compacted.push({ ...action, subjects: [...action.subjects] });
  }
  return compacted;
}

function orderedUnique(subjects: string[]) {
  return subjects.filter((subject, index) => subjects.indexOf(subject) === index);
}

export function presentationKind(presentation: ToolPresentation): PresentationDisplayKind {
  const kinds = new Set(presentation.actions.map((action) => action.kind));
  return kinds.size === 1 ? presentation.actions[0]?.kind ?? "inspect" : "inspect";
}

export function presentationSubject(presentation: ToolPresentation) {
  if (presentationKind(presentation) === "inspect") return "files";
  if (presentation.actions.length > 1) return "searches";
  const action = presentation.actions[0];
  if (!action) return undefined;
  if (action.kind === "search") {
    const scopes = action.scopes.map(displayWorkspace).filter(Boolean);
    const query = `“${action.query.trim()}”`;
    return scopes.length ? `${query} in ${naturalJoin(scopes)}` : query;
  }
  const subjects = action.subjects.map((subject) => subject.trim()).filter(Boolean);
  if (!subjects.length) return undefined;
  const joined = naturalJoin(subjects);
  return action.kind === "skill"
    ? `${joined} ${subjects.length === 1 ? "skill" : "skills"}`
    : joined;
}

export function semanticTitleText(title: ActivityStepSemanticTitle) {
  return title.actions.map((action) => {
    const subjects = naturalJoin(action.subjects);
    const actionAndSubjects = subjects ? `${action.action} ${subjects}` : action.action;
    return action.scope ? `${actionAndSubjects} in ${action.scope}` : actionAndSubjects;
  }).join("; ");
}

function summarizeSubjects(subjects: string[]) {
  return subjects.length <= 3
    ? subjects
    : [subjects[0], subjects[1], `${subjects.length - 2} more`];
}

function appendSkillNoun(subjects: string[], fullCount: number) {
  return subjects.map((subject, index) =>
    index === subjects.length - 1
      ? `${subject} ${fullCount === 1 ? "skill" : "skills"}`
      : subject
  );
}

function displayWorkspace(scope: string) {
  return scope.trim() === "." ? "workspace" : scope.trim();
}

/** Uses the shortest path suffix that still distinguishes every visible scope. */
function shortestUniqueSuffixes(scopes: string[]) {
  const result = [...scopes];
  const pathIndexes = scopes
    .map((scope, index) => ({ scope, index }))
    .filter(({ scope }) => scope !== "workspace");
  const parts = new Map(pathIndexes.map(({ scope, index }) => [
    index,
    scope.split(/[\\/]+/).filter(Boolean),
  ]));

  for (const { index } of pathIndexes) {
    const segments = parts.get(index) ?? [];
    let length = 1;
    while (length < segments.length) {
      const suffix = segments.slice(-length).join("/");
      const collision = pathIndexes.some(({ index: otherIndex }) => {
        if (otherIndex === index) return false;
        const other = parts.get(otherIndex) ?? [];
        return other.slice(-length).join("/") === suffix;
      });
      if (!collision) break;
      length += 1;
    }
    result[index] = segments.slice(-length).join("/");
  }
  return result;
}

function naturalJoin(values: string[]) {
  if (values.length < 2) return values[0] ?? "";
  if (values.length === 2) return `${values[0]} and ${values[1]}`;
  return `${values.slice(0, -1).join(", ")}, and ${values.at(-1)}`;
}
