import type { ActivityToolDetails } from "@openaide/app-shell-contracts";
import { firstFieldValue } from "../state/toolDetailsShared";
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
  // Agent-native activations pass the request to the skill as `args` and return only a confirmation.
  const args = firstFieldValue(details.input?.fields, "args")?.trim();

  return (
    <div className="activity-tool-details skill-tool-details">
      {args ? <p className="skill-tool-args">{args}</p> : null}
      {skill.body ? (
        <AgentMarkdown className="chat-agent skill-tool-markdown" text={skill.body} />
      ) : args ? null : (
        <p className="activity-tool-muted">No skill instructions returned.</p>
      )}
    </div>
  );
}
