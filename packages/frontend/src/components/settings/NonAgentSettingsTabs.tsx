import type {
  McpServerSettingsRecord,
  SettingsProjectionAvailability,
  SkillSettingsRecord,
} from "@openaide/app-shell-contracts";
import { EmptySettingsState, InlineFailure, SettingsSkeleton, StatusBadge } from "./settingsPresentation";

export function McpSettingsTab({
  error,
  availability,
  loading,
  servers,
}: {
  error?: string;
  availability?: SettingsProjectionAvailability;
  loading?: boolean;
  servers?: McpServerSettingsRecord[];
}) {
  if (loading && !servers) return <SettingsSkeleton />;
  if (error && !servers) return <InlineFailure message={error} />;
  if (availability === "unavailable") {
    return (
      <EmptySettingsState
        title="MCP discovery unavailable"
        detail="OpenAIDE cannot currently enumerate MCP servers from the App Server."
      />
    );
  }
  if (!servers?.length) {
    return (
      <EmptySettingsState
        title="No MCP servers"
        detail="Agent configuration has not exposed MCP servers for this workspace."
      />
    );
  }
  return (
    <div className="settings-list">
      {error ? <InlineFailure message={error} muted /> : null}
      {servers.map((server) => (
        <article className="settings-record compact readonly" key={server.id}>
          <header>
            <span className="settings-record-heading">
              <strong>{server.label}</strong>
              <small>{server.description ?? server.id}</small>
            </span>
            <StatusBadge status={server.enabled ? server.status : "disabled"} />
          </header>
          <dl>
            <dt>Scope</dt>
            <dd>{server.scope}</dd>
            <dt>Transport</dt>
            <dd>{server.transport}</dd>
            <dt>Tools</dt>
            <dd>{server.tool_count ?? "Unknown"}</dd>
            {server.last_error_summary ? (
              <>
                <dt>Last error</dt>
                <dd>{server.last_error_summary}</dd>
              </>
            ) : null}
          </dl>
        </article>
      ))}
    </div>
  );
}

export function SkillsSettingsTab({
  error,
  availability,
  loading,
  skills,
}: {
  error?: string;
  availability?: SettingsProjectionAvailability;
  loading?: boolean;
  skills?: SkillSettingsRecord[];
}) {
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
    <div className="settings-list">
      {error ? <InlineFailure message={error} muted /> : null}
      {skills.map((skill) => (
        <article className="settings-record compact readonly" key={skill.id}>
          <header>
            <span className="settings-record-heading">
              <strong>{skill.label}</strong>
              <small>{skill.description ?? skill.source_label}</small>
            </span>
            <StatusBadge status={skill.status} />
          </header>
          <dl>
            <dt>Scope</dt>
            <dd>{skill.scope}</dd>
            <dt>Source</dt>
            <dd>{skill.source_label}</dd>
            <dt>Last scanned</dt>
            <dd>{skill.last_scanned_at}</dd>
            {skill.tags.length ? (
              <>
                <dt>Tags</dt>
                <dd>{skill.tags.join(", ")}</dd>
              </>
            ) : null}
          </dl>
          {skill.warnings.map((warning) => (
            <InlineFailure key={warning} message={warning} muted />
          ))}
        </article>
      ))}
    </div>
  );
}
