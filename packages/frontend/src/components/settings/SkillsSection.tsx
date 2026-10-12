import type {
  SettingsProjectionAvailability,
  SkillSettingsDetails,
  SkillSettingsRecord,
} from "@openaide/app-shell-contracts";
import { FileText, Sparkles } from "lucide-react";
import { useRef, useState } from "react";
import { AgentMarkdown } from "../AgentMarkdown";
import { settingsInfo } from "./settingsInfo";
import {
  errorSentence,
  SettingsBlankState,
  SettingsCrumb,
  SettingsHealth,
  SettingsIdentity,
  SettingsInlineError,
  SettingsRowView,
  SettingsSegmented,
} from "./settingsKit";
import type { SettingsSection } from "./settingsModel";

export type SkillsSectionProps = {
  available: boolean;
  availability?: SettingsProjectionAvailability;
  error?: string;
  loading?: boolean;
  onLoadSkill?: (id: string) => Promise<SkillSettingsDetails>;
  skills?: SkillSettingsRecord[];
};

/** How many front-matter fields a Skill page shows before folding the rest. */
const visibleFieldLimit = 5;

/** Skills are files on disk; Settings shows what Agents will read and never edits it. */
export function useSkillsSection({ available, availability, error, loading, onLoadSkill, skills }: SkillsSectionProps): SettingsSection | undefined {
  const [selected, setSelected] = useState<SkillSettingsRecord>();
  const [details, setDetails] = useState<SkillSettingsDetails>();
  const [detailsError, setDetailsError] = useState<string>();
  const generation = useRef(0);
  if (!available) return undefined;

  const openSkill = async (skill: SkillSettingsRecord) => {
    if (!onLoadSkill) return;
    const current = ++generation.current;
    setSelected(skill);
    setDetails(undefined);
    setDetailsError(undefined);
    try {
      const loaded = await onLoadSkill(skill.id);
      if (current === generation.current) setDetails(loaded);
    } catch (cause) {
      if (current === generation.current) setDetailsError(errorSentence(cause, "Unable to read this skill."));
    }
  };
  const closeDetail = () => {
    generation.current += 1;
    setSelected(undefined);
    setDetails(undefined);
    setDetailsError(undefined);
    return true;
  };
  const unavailable = availability === "unavailable";

  return {
    id: "skills",
    label: "Skills",
    icon: Sparkles,
    info: settingsInfo.skills,
    notice: (
      <>
        {unavailable ? <SettingsBlankState detail="OpenAIDE cannot read installed Skills from the App Server." title="Skills unavailable" />
          : !skills && loading ? <SettingsBlankState title="Loading skills…" />
            : skills && !skills.length ? <SettingsBlankState detail="Agents found no Skills for this workspace." title="No skills" /> : null}
        {error && !unavailable ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      </>
    ),
    groups: unavailable ? [] : groupSkills(skills ?? []).map((group) => ({
      id: group.key,
      title: group.label,
      rows: group.skills.map((skill) => ({
        id: skill.id,
        label: skill.label,
        description: skill.description ?? skill.source_label,
        keywords: `skill ${skill.source_label} ${skill.tags.join(" ")}`,
        leading: <Sparkles aria-hidden="true" size={18} strokeWidth={1.6} />,
        status: skill.status === "valid" ? undefined : { tone: "warn" as const, text: skillStatusLabel(skill.status) },
        onOpen: onLoadSkill ? () => void openSkill(skill) : undefined,
      })),
    })),
    closeDetail: selected ? closeDetail : undefined,
    detail: selected ? <SkillPage details={details} error={detailsError} onBack={closeDetail} skill={selected} /> : undefined,
  };
}

function SkillPage({
  details,
  error,
  onBack,
  skill,
}: {
  details?: SkillSettingsDetails;
  error?: string;
  onBack: () => void;
  skill: SkillSettingsRecord;
}) {
  const [view, setView] = useState<"rendered" | "source">("rendered");
  const [allFields, setAllFields] = useState(false);
  const fields = details?.document.additional_fields ?? [];
  const shown = allFields ? fields : fields.slice(0, visibleFieldLimit);
  return (
    <article aria-label="Skill details" className="settings-detail">
      <SettingsCrumb label="Skills" onBack={onBack} />
      <SettingsIdentity
        icon={<Sparkles aria-hidden="true" size={24} strokeWidth={1.5} />}
        subtitle={skill.scope === "global" ? "All projects" : skill.source_label}
        title={details?.document.name ?? skill.label}
      />
      {skill.status !== "valid" || skill.warnings.length ? (
        <SettingsHealth
          status={skillStatusLabel(skill.status)}
          text={skill.warnings.length ? skill.warnings.join(" ") : undefined}
          tone="warn"
        />
      ) : null}
      {error ? <SettingsInlineError>{error}</SettingsInlineError> : null}
      {!details && !error ? <SettingsBlankState title="Loading skill…" /> : null}
      {details ? (
        <>
          {details.document.description ? <p className="settings-lede">{details.document.description}</p> : null}
          {fields.length ? (
            <section className="settings-group">
              <header><h2>Settings</h2></header>
              <dl className="settings-card settings-facts">
                {shown.map((field) => (
                  <div key={field.name}><dt className="settings-mono">{field.name}</dt><dd>{field.value}</dd></div>
                ))}
                {fields.length > visibleFieldLimit ? (
                  <button aria-expanded={allFields} onClick={() => setAllFields(!allFields)} type="button">
                    {allFields ? "Show fewer" : `Show all ${fields.length}`}
                  </button>
                ) : null}
              </dl>
            </section>
          ) : null}
          <section className="settings-group">
            <header><h2>File</h2></header>
            {/* TODO: the Skill projection carries a source label, not a path. Add the path to
                the App Server Protocol before offering Open folder here; a button without it would do nothing. */}
            <ul className="settings-card">
              <SettingsRowView row={{ id: "file", label: "SKILL.md", description: skill.source_label, leading: <FileText aria-hidden="true" size={18} strokeWidth={1.6} /> }} />
            </ul>
          </section>
          <section className="settings-group">
            <header>
              <h2>Instructions</h2>
              <span className="settings-group-actions">
                <SettingsSegmented
                  label="Instructions view"
                  onChange={setView}
                  options={[{ value: "rendered", label: "Rendered" }, { value: "source", label: "Source" }]}
                  value={view}
                />
              </span>
            </header>
            {view === "source" ? (
              <pre className="settings-card settings-source settings-mono">{details.document.source}</pre>
            ) : details.document.instructions.trim() ? (
              <div className="settings-card settings-prose"><AgentMarkdown className="settings-markdown" text={details.document.instructions} /></div>
            ) : <p className="settings-empty">None.</p>}
          </section>
        </>
      ) : null}
    </article>
  );
}

function skillStatusLabel(status: SkillSettingsRecord["status"]) {
  if (status === "shadowed") return "Shadowed";
  if (status === "invalid") return "Invalid";
  if (status === "warning") return "Warning";
  return "Valid";
}

function groupSkills(skills: SkillSettingsRecord[]) {
  const groups = new Map<string, { key: string; label: string; skills: SkillSettingsRecord[] }>();
  for (const skill of skills) {
    const key = skill.scope === "global" ? "global" : `workspace:${skill.source_label}`;
    const existing = groups.get(key);
    if (existing) existing.skills.push(skill);
    else groups.set(key, { key, label: skill.scope === "global" ? "Global" : skill.source_label, skills: [skill] });
  }
  return [...groups.values()];
}
