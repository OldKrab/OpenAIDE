import type {
  McpServerSettingsRecord,
  SkillSettingsDetails,
  SkillSettingsRecord,
} from "@openaide/app-shell-contracts";
import type {
  SettingsMcpServerRecord,
  SettingsMcpServersResult,
  SettingsSection,
  SettingsSkillRecord,
  SettingsSkillDetailsResult,
  SettingsSkillsResult,
} from "@openaide/app-server-client";

export function mapSettingsSections(sections: SettingsSection[]) {
  return sections.map(mapSettingsSection).filter((section) => section !== undefined);
}

export function mapMcpServersProjection(result: SettingsMcpServersResult) {
  return {
    generatedAt: result.generatedAt,
    availability: result.availability,
    servers: result.servers.map(mapMcpServer),
  };
}

export function mapSkillsProjection(result: SettingsSkillsResult) {
  return {
    generatedAt: result.generatedAt,
    availability: result.availability,
    skills: result.skills.map(mapSkill),
  };
}

export function mapSkillDetailsProjection(result: SettingsSkillDetailsResult): SkillSettingsDetails {
  return {
    generated_at: result.generatedAt,
    skill: mapSkill(result.skill),
    document: {
      name: result.document.name,
      description: result.document.description,
      additional_fields: (result.document.additionalFields ?? []).map((field) => ({
        name: field.name,
        value: field.value,
      })),
      instructions: result.document.instructions,
      source: result.document.source,
    },
  };
}

function mapSettingsSection(section: SettingsSection) {
  switch (section) {
    case "agents":
      return "agents";
    case "mcpServers":
      return "mcp";
    case "skills":
      return "skills";
    case "commonSettings":
      return "common";
  }
}

function mapMcpServer(server: SettingsMcpServerRecord): McpServerSettingsRecord {
  return {
    id: server.id,
    label: server.label,
    enabled: server.enabled,
    scope: server.scope,
    transport: server.transport,
    status: server.status,
    description: server.description ?? undefined,
    validation_error: server.validationError ?? undefined,
  };
}

function mapSkill(skill: SettingsSkillRecord): SkillSettingsRecord {
  return {
    id: skill.id,
    label: skill.label,
    scope: skill.scope,
    source_label: skill.sourceLabel,
    status: skill.status,
    description: skill.description ?? undefined,
    warnings: skill.warnings ?? [],
    tags: skill.tags ?? [],
    last_scanned_at: skill.lastScannedAt,
  };
}
