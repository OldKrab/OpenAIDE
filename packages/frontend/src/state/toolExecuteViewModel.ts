import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { activityPresentationLabel, activityPresentationStatus } from "./activityLabels";
import { displayCommand } from "./toolCommandViewModel";
import { firstFieldValue } from "./toolDetailsShared";

export type ExecuteOutput = {
  label: "stdout" | "stderr" | "Combined output" | "preview";
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
  const outputs: ExecuteOutput[] = [];
  if (stdout) outputs.push({ label: "stdout", text: stdout, tone: "stdout" });
  if (stderr) outputs.push({ label: "stderr", text: stderr, tone: "stderr" });
  if (outputs.length === 0) {
    // These fields are alternate merged representations, not extra streams.
    const combined = aggregated || formatted;
    if (combined) outputs.push({ label: "Combined output", text: combined, tone: "stdout" });
  }
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
