import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { activityPresentationLabel, activityPresentationStatus } from "./activityLabels";
import { displayCommand } from "./toolCommandViewModel";
import { firstFieldValue } from "./toolDetailsShared";

export type ExecuteOutput = {
  label: "stdout" | "stderr" | "aggregate" | "formatted" | "preview";
  text: string;
  tone: "stdout" | "stderr";
};

export function executeDetailInfo(
  details: ActivityToolDetails,
  step: Extract<ActivityStep, { kind: "tool" }>,
  fallbackPreview?: string,
) {
  const command = displayCommand(details.input?.command) ?? step.input_summary ?? "command";
  const stdout = details.output?.stdout;
  const stderr = details.output?.stderr;
  const formatted = details.output?.formatted_output;
  const aggregated = details.output?.aggregated_output;
  const exitCode = details.output?.exit_code;
  const mode = activityPresentationStatus(step.status);
  const outputs = distinctOutputs([
    { label: "stdout", text: stdout, tone: "stdout" },
    { label: "stderr", text: stderr, tone: "stderr" },
    { label: "aggregate", text: aggregated, tone: "stdout" },
    { label: "formatted", text: formatted, tone: "stdout" },
  ]);
  if (outputs.length === 0 && fallbackPreview) {
    outputs.push({ label: "preview", text: fallbackPreview, tone: "stdout" });
  }
  return {
    command,
    duration: firstFieldValue(details.output?.fields, "duration"),
    exitCode,
    failed: mode === "failed",
    mode,
    outputs,
    resultLabel: activityPresentationLabel(step.status),
  };
}

function distinctOutputs(candidates: Array<Omit<ExecuteOutput, "text"> & { text?: string }>): ExecuteOutput[] {
  const seen = new Set<string>();
  return candidates.flatMap((candidate) => {
    if (!candidate.text) return [];
    const isAlias = candidate.label === "aggregate" || candidate.label === "formatted";
    if (isAlias && seen.has(candidate.text)) return [];
    seen.add(candidate.text);
    return [{ ...candidate, text: candidate.text }];
  });
}
