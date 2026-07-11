import type { ActivityToolDetails } from "@openaide/app-shell-contracts";
import { readDetailOutput } from "../state/toolDetailsViewModel";
import { parseSkillDocument } from "../state/skillToolViewModel";
import { AgentMarkdown } from "./AgentMarkdown";

export function SkillToolDetails({
  details,
  fallbackPreview,
}: {
  details: ActivityToolDetails;
  fallbackPreview?: string;
}) {
  const content = readDetailOutput(details, fallbackPreview);
  const skill = parseSkillDocument(content);

  return (
    <div className="activity-tool-details skill-tool-details">
      {skill.body ? (
        <AgentMarkdown className="chat-agent skill-tool-markdown" text={skill.body} />
      ) : (
        <p className="activity-tool-muted">No skill instructions returned.</p>
      )}
    </div>
  );
}
