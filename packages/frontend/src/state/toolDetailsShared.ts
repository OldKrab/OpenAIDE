import type {
  ActivityStep,
  ActivityToolContent,
  ActivityToolDetails,
  ActivityToolField,
  ActivityToolInput,
  ActivityToolOutput,
} from "@openaide/app-shell-contracts";

export function readDetailPath(details: ActivityToolDetails) {
  return firstToolPath(details)?.path ?? details.input?.path ?? "";
}

export function readDetailOutput(details: ActivityToolDetails, fallbackPreview?: string) {
  const textContent = details.content?.find((content) => content.kind === "text");
  if (textContent?.kind === "text") return textContent.text;
  return primaryOutput(details.output) ?? fallbackPreview ?? "";
}

export function firstFieldValue(fields: ActivityToolField[] | undefined, name: string) {
  return fields?.find((field) => field.name.toLowerCase() === name.toLowerCase())?.value;
}

export function hasToolDetails(step: Extract<ActivityStep, { kind: "tool" }>) {
  const details = step.details;
  if (step.detail_artifact_id) return true;
  if (step.output_preview) return true;
  return Boolean(
    details &&
      ((details.locations?.length ?? 0) > 0 ||
        (details.content?.length ?? 0) > 0 ||
        details.input ||
        details.output),
  );
}

export function hasToolInput(input: ActivityToolInput) {
  return Boolean(input.command?.length || input.cwd || input.path || input.query || input.url || input.fields?.length);
}

export function hasToolOutputBody(output: ActivityToolOutput) {
  return Boolean(primaryOutput(output) || output.stderr || output.fields?.length);
}

export function firstToolPath(details: NonNullable<Extract<ActivityStep, { kind: "tool" }>["details"]>) {
  const firstLocation = details.locations?.[0];
  if (firstLocation) return firstLocation;
  const firstDiff = details.content?.find((content): content is Extract<ActivityToolContent, { kind: "diff" }> => content.kind === "diff");
  if (firstDiff) return { path: firstDiff.path, line: undefined };
  if (details.input?.path) return { path: details.input.path, line: undefined };
  return undefined;
}

export function primaryOutput(output?: ActivityToolOutput) {
  if (!output) return undefined;
  return output.formatted_output || output.aggregated_output || output.stdout;
}

export function filteredOutputFields(details: ActivityToolDetails) {
  const fields = details.output?.fields ?? [];
  return fields.filter((field) => {
    const name = field.name.toLowerCase();
    const value = field.value.toLowerCase();
    if (name === "cwd" && field.value === details.input?.cwd) return false;
    if (name === "status" && ["completed", "success", "succeeded"].includes(value)) return false;
    if (name === "success" && ["true", "yes", "1"].includes(value)) return false;
    return true;
  });
}

export function toolKindClass(kind: string) {
  return kind.replace(/[^a-z0-9_-]/gi, "-").toLowerCase() || "other";
}

/** Preserves legacy activity while selecting a renderer from loaded typed details. */
export function toolPresentationName(name: string, details?: ActivityToolDetails) {
  const toolType = firstFieldValue(details?.input?.fields, "type");
  if (name === "search" && toolType?.toLowerCase() === "websearch") return "web_search";
  return name;
}
