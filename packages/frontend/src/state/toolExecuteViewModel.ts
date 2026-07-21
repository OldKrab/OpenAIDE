import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { displayCommand } from "./toolCommandViewModel";
import { firstFieldValue } from "./toolDetailsShared";

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
  const terminal = details.terminal_outputs?.map((item) => item.output).join("");
  const exitCode = details.output?.exit_code;
  const running = step.status === "running";
  const failed = step.status === "error" || details.output?.success === false || (exitCode !== undefined && exitCode !== 0);
  const outputText = failed
    ? stderr || terminal || aggregated || formatted || stdout || fallbackPreview
    : terminal || stdout || formatted || aggregated || fallbackPreview;
  return {
    command,
    duration: firstFieldValue(details.output?.fields, "duration"),
    exitCode,
    failed,
    mode: running ? "running" : failed ? "failed" : "completed",
    outputLabel: failed && stderr ? "stderr" : "stdout",
    outputText,
    outputTone: failed && stderr ? "stderr" : "stdout",
  };
}
