import type { ActivityStep, ActivityToolDetails } from "@openaide/app-shell-contracts";
import type { ToolImagePreview } from "@openaide/app-server-client";
import { useEffect, useState } from "react";
import { ToolCodeBlock } from "./ChatToolBlocks";
import { EditToolDetails } from "./EditToolDetails";
import { DefinedToolDetails } from "./DefinedToolDetails";
import { ExecuteToolDetails } from "./ExecuteToolDetails";
import { GenericToolStepDetails } from "./GenericToolDetails";
import { ReadToolDetails } from "./ReadToolDetails";
import { SearchToolDetails } from "./SearchToolDetails";
import { SkillToolDetails } from "./SkillToolDetails";
import { WebSearchToolDetails } from "./WebSearchToolDetails";
import { ToolImageFilePreview } from "./ToolImageFilePreview";

export function ChatToolDetails({
  details,
  error,
  fallbackPreview,
  imagePreview,
  loading,
  step,
}: {
  details?: ActivityToolDetails;
  error?: string;
  fallbackPreview?: string;
  imagePreview?: ToolImagePreview;
  loading?: boolean;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  if (!details) {
    if (step.name === "execute") return <ExecuteToolDetails details={emptyToolDetails} fallbackPreview={fallbackPreview} step={step} />;
    if (loading) return <DelayedToolDetailsSkeleton />;
    if (error) return <p className="activity-tool-muted">{error}</p>;
    return fallbackPreview ? <ToolCodeBlock text={fallbackPreview} /> : null;
  }
  if (step.name === "read") {
    return (
      <ReadToolDetails
        details={details}
        fallbackPreview={fallbackPreview}
        imagePreview={imagePreview}
        step={step}
      />
    );
  }
  let content = <GenericToolStepDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  if (step.name === "skill") content = <SkillToolDetails details={details} fallbackPreview={fallbackPreview} />;
  else if (step.name === "edit") content = <EditToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  else if (step.name === "search") content = <SearchToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  else if (step.name === "web_search") content = <WebSearchToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  else if (step.name === "execute") content = <ExecuteToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  else if (["delete", "move", "think", "fetch", "switch_mode"].includes(step.name)) {
    content = <DefinedToolDetails details={details} fallbackPreview={fallbackPreview} step={step} />;
  }
  return imagePreview ? <>{content}<ToolImageFilePreview preview={imagePreview} /></> : content;
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
