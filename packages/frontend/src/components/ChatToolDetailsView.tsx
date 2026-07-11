import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { useEffect, useState } from "react";
import { ToolCodeBlock } from "./ChatToolBlocks";
import { EditToolDetails } from "./EditToolDetails";
import { ExecuteToolDetails } from "./ExecuteToolDetails";
import { GenericToolStepDetails } from "./GenericToolDetails";
import { ReadToolDetails } from "./ReadToolDetails";
import { SearchToolDetails } from "./SearchToolDetails";
import { SkillToolDetails } from "./SkillToolDetails";
import { WebSearchToolDetails } from "./WebSearchToolDetails";

export function ChatToolDetails({
  details,
  error,
  fallbackPreview,
  loading,
  step,
}: {
  details?: ActivityToolDetails;
  error?: string;
  fallbackPreview?: string;
  loading?: boolean;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  if (!details) {
    if (step.name === "execute") return <ExecuteToolDetails details={emptyToolDetails} fallbackPreview={fallbackPreview} step={step} />;
    if (loading) return <DelayedToolDetailsSkeleton />;
    if (error) return <p className="activity-tool-muted">{error}</p>;
    return fallbackPreview ? <ToolCodeBlock text={fallbackPreview} /> : null;
  }
  if (step.name === "skill") return <SkillToolDetails details={details} fallbackPreview={fallbackPreview} />;
  if (step.name === "read") return <ReadToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  if (step.name === "edit") return <EditToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  if (step.name === "search") return <SearchToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  if (step.name === "web_search") return <WebSearchToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  if (step.name === "execute") return <ExecuteToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  return <GenericToolStepDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
}

function DelayedToolDetailsSkeleton() {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const id = setTimeout(() => setVisible(true), 150);
    return () => clearTimeout(id);
  }, []);
  if (!visible) return null;
  return (
    <div className="activity-tool-skeleton" aria-hidden="true">
      <span />
      <span />
      <span />
    </div>
  );
}

const emptyToolDetails: ActivityToolDetails = {
  locations: [],
  content: [],
};
