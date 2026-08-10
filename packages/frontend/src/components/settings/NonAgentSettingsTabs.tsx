import type {
  SettingsProjectionAvailability,
  SkillSettingsDetails,
  SkillSettingsRecord,
} from "@openaide/app-shell-contracts";
import { ChevronLeft, ChevronRight, Sparkles } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { AgentMarkdown } from "../AgentMarkdown";
import { SettingsCatalogSearch } from "./SettingsCatalogSearch";
import { EmptySettingsState, InlineFailure, SettingsSkeleton, StatusBadge } from "./settingsPresentation";

export function SkillsSettingsTab({
  error,
  availability,
  loading,
  onLoadSkill,
  skills,
}: {
  error?: string;
  availability?: SettingsProjectionAvailability;
  loading?: boolean;
  onLoadSkill?: (id: string) => Promise<SkillSettingsDetails>;
  skills?: SkillSettingsRecord[];
}) {
  const [selectedSkill, setSelectedSkill] = useState<SkillSettingsRecord>();
  const [details, setDetails] = useState<SkillSettingsDetails>();
  const [detailsError, setDetailsError] = useState<string>();
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [query, setQuery] = useState("");
  const loadGeneration = useRef(0);
  const visibleSkills = useMemo(() => filterSkills(skills ?? [], query), [query, skills]);
  const openSkill = async (skill: SkillSettingsRecord) => {
    if (!onLoadSkill) return;
    const generation = ++loadGeneration.current;
    setSelectedSkill(skill);
    setDetails(undefined);
    setDetailsError(undefined);
    setDetailsLoading(true);
    try {
      const loaded = await onLoadSkill(skill.id);
      if (generation === loadGeneration.current) setDetails(loaded);
    } catch (loadError) {
      if (generation === loadGeneration.current) {
        setDetailsError(loadError instanceof Error ? loadError.message : "Unable to read this skill.");
      }
    } finally {
      if (generation === loadGeneration.current) setDetailsLoading(false);
    }
  };
  const closeSkill = () => {
    loadGeneration.current += 1;
    setSelectedSkill(undefined);
    setDetails(undefined);
    setDetailsError(undefined);
    setDetailsLoading(false);
  };

  if (selectedSkill) {
    return (
      <SkillDocumentView
        details={details}
        error={detailsError}
        loading={detailsLoading}
        onBack={closeSkill}
        skill={selectedSkill}
      />
    );
  }
  if (loading && !skills) return <SettingsSkeleton />;
  if (error && !skills) return <InlineFailure message={error} />;
  if (availability === "unavailable") {
    return (
      <EmptySettingsState
        title="Skills discovery unavailable"
        detail="OpenAIDE cannot currently enumerate installed skills from the App Server."
      />
    );
  }
  if (!skills?.length) {
    return (
      <EmptySettingsState
        title="No skills"
        detail="Agent configuration has not exposed skills for this workspace."
      />
    );
  }
  return (
    <div className="skill-settings-page">
      <div className="skill-library-intro">
        <p>Skills teach agents repeatable ways to work. Open one to understand when it applies and the instructions it adds.</p>
        <SettingsCatalogSearch
          label="Search skills"
          onChange={setQuery}
          placeholder="Search"
          value={query}
        />
      </div>
      {error ? <InlineFailure message={error} muted /> : null}
      {!visibleSkills.length ? (
        <EmptySettingsState title="No skills found" detail="No skills match this search." />
      ) : (
        <div className="skill-settings-groups">
          {groupSkills(visibleSkills).map((group) => (
            <section className="skill-settings-group" key={group.key}>
              <header>
                <strong>{group.label}</strong>
              </header>
              <div className="skill-settings-list">
                {group.skills.map((skill) => (
                  <button
                    className="skill-settings-row"
                    disabled={!onLoadSkill}
                    key={skill.id}
                    onClick={() => void openSkill(skill)}
                    type="button"
                  >
                    <span className="skill-settings-icon"><Sparkles aria-hidden="true" size={16} /></span>
                    <span className="skill-settings-copy">
                      <strong>{skill.label}</strong>
                      <small>{skill.description ?? skill.source_label}</small>
                    </span>
                    <span className="skill-settings-origin">{skill.source_label}</span>
                    <span className="skill-settings-status-slot">{skill.status === "valid" ? null : <StatusBadge status={skill.status} />}</span>
                    <ChevronRight aria-hidden="true" size={15} />
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}

function filterSkills(skills: SkillSettingsRecord[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return skills;
  return skills.filter((skill) => (
    `${skill.label} ${skill.description ?? ""} ${skill.source_label} ${skill.tags.join(" ")}`
      .toLowerCase()
      .includes(normalized)
  ));
}

function groupSkills(skills: SkillSettingsRecord[]) {
  const groups = new Map<string, { key: string; label: string; skills: SkillSettingsRecord[] }>();
  for (const skill of skills) {
    const key = skill.scope === "global" ? "global" : `workspace:${skill.source_label}`;
    const existing = groups.get(key);
    if (existing) {
      existing.skills.push(skill);
    } else {
      groups.set(key, {
        key,
        label: skill.scope === "global" ? "Global" : skill.source_label,
        skills: [skill],
      });
    }
  }
  return [...groups.values()];
}

function SkillDocumentView({
  details,
  error,
  loading,
  onBack,
  skill,
}: {
  details?: SkillSettingsDetails;
  error?: string;
  loading: boolean;
  onBack: () => void;
  skill: SkillSettingsRecord;
}) {
  return (
    <article className="skill-document">
      <button
        aria-label="Back to Skills"
        className="settings-detail-back"
        onClick={onBack}
        type="button"
      >
        <ChevronLeft aria-hidden="true" size={15} />
        Back to Skills
      </button>
      <header className="skill-document-header">
        <span className="skill-document-icon"><Sparkles aria-hidden="true" size={20} /></span>
        <span className="skill-document-identity">
          <h2>{details?.document.name ?? skill.label}</h2>
          <small>{skill.source_label}</small>
        </span>
        {skill.status === "valid" ? null : <StatusBadge status={skill.status} />}
      </header>
      {loading ? <SettingsSkeleton /> : null}
      {error ? <InlineFailure message={error} /> : null}
      {details ? (
        <div className="skill-document-content">
          <section>
            <h3>Description</h3>
            <p>{details.document.description}</p>
          </section>
          {details.document.additional_fields.length ? (
            <section>
              <h3>Metadata</h3>
              <dl className="skill-document-metadata">
                {details.document.additional_fields.map((field) => (
                  <div key={field.name}>
                    <dt>{field.name}</dt>
                    <dd><pre>{field.value}</pre></dd>
                  </div>
                ))}
              </dl>
            </section>
          ) : null}
          <section>
            <h3>Instructions</h3>
            {details.document.instructions.trim() ? (
              <AgentMarkdown className="skill-document-instructions" text={details.document.instructions} />
            ) : (
              <p className="skill-document-empty">No instructions.</p>
            )}
          </section>
          <details className="skill-document-source">
            <summary>View source</summary>
            <pre>{details.document.source}</pre>
          </details>
          {skill.warnings.map((warning) => (
            <InlineFailure key={warning} message={warning} muted />
          ))}
        </div>
      ) : null}
    </article>
  );
}
