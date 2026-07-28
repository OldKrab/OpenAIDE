import type { ActivityStep } from "@openaide/app-shell-contracts";

type ToolPresentation = NonNullable<Extract<ActivityStep, { kind: "tool" }>["presentation"]>;

export type ActivityStepSemanticTitle = {
  actions: ActivityStepSemanticAction[];
  /** Full, safe semantic title used when compact row text is elided. */
  tooltip: string;
};

export type ActivityStepSemanticAction = {
  action: "Activated" | "Read" | "Search" | "Search file names for" | "View";
  scope?: string;
  subjects: string[];
};

export type PresentationDisplayKind =
  ToolPresentation["actions"][number]["kind"] | "inspect";

export function presentationSemanticTitle(
  presentation: ToolPresentation,
): ActivityStepSemanticTitle | undefined {
  const compactActions: ActivityStepSemanticAction[] = [];
  const fullActions: ActivityStepSemanticAction[] = [];

  for (const action of presentation.actions) {
    if (action.kind === "search") {
      const query = action.query.trim();
      if (!query) return undefined;
      const scopes = action.scopes.map(displayWorkspace).filter(Boolean);
      const compactScopes = shortestUniqueSuffixes(scopes);
      const label = action.target === "paths" ? "Search file names for" : "Search";
      compactActions.push({
        action: label,
        subjects: [`“${query}”`],
        ...(compactScopes.length ? { scope: naturalJoin(compactScopes) } : {}),
      });
      fullActions.push({
        action: label,
        subjects: [`“${query}”`],
        ...(scopes.length ? { scope: naturalJoin(scopes) } : {}),
      });
      continue;
    }

    const mixedSkill = action.kind === "skill" && presentation.actions.length > 1;
    if (!mixedSkill && action.kind !== "read" && action.kind !== "view") return undefined;
    const subjects = action.subjects.map((subject) => subject.trim()).filter(Boolean);
    if (!subjects.length) return undefined;
    const verb = mixedSkill ? "Activated" : action.kind === "view" ? "View" : "Read";
    compactActions.push({
      action: verb,
      subjects: mixedSkill
        ? appendSkillNoun(summarizeSubjects(subjects), subjects.length)
        : summarizeSubjects(subjects),
    });
    fullActions.push({
      action: verb,
      subjects: mixedSkill ? appendSkillNoun(subjects, subjects.length) : subjects,
    });
  }

  if (compactActions.length !== presentation.actions.length) return undefined;
  return {
    actions: compactActions,
    tooltip: semanticTitleText({ actions: fullActions, tooltip: "" }),
  };
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
