export type SkillDocument = {
  name?: string;
  description?: string;
  body: string;
};

export function parseSkillDocument(content: string): SkillDocument {
  const normalized = content.replace(/\r\n?/g, "\n");
  if (!normalized.startsWith("---\n")) return { body: normalized };

  const closingDelimiter = normalized.indexOf("\n---\n", 4);
  if (closingDelimiter < 0) return { body: normalized };

  const frontmatter = normalized.slice(4, closingDelimiter);
  return {
    ...optionalField("name", frontmatter),
    ...optionalField("description", frontmatter),
    body: normalized.slice(closingDelimiter + 5).trimStart(),
  };
}

function optionalField(field: "name" | "description", frontmatter: string) {
  const line = frontmatter.split("\n").find((candidate) => candidate.startsWith(`${field}:`));
  const value = line ? scalarValue(line.slice(field.length + 1)) : undefined;
  return value ? { [field]: value } : {};
}

function scalarValue(rawValue: string) {
  const value = rawValue.trim();
  if (value.length < 2) return value || undefined;
  if (value.startsWith('"') && value.endsWith('"')) {
    try {
      return JSON.parse(value) as string;
    } catch {
      return value.slice(1, -1);
    }
  }
  if (value.startsWith("'") && value.endsWith("'")) return value.slice(1, -1).replace(/''/g, "'");
  return value;
}
