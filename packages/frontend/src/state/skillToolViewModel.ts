export type SkillDocument = {
  name?: string;
  description?: string;
  body: string;
};

/** Reads the stable skill identity even when a live tool preview is truncated before its closing delimiter. */
export function skillDocumentName(content: string) {
  const normalized = content.replace(/\r\n?/g, "\n");
  if (!normalized.startsWith("---\n")) return undefined;
  const closingDelimiter = normalized.indexOf("\n---\n", 4);
  const frontmatter = closingDelimiter < 0
    ? normalized.slice(4)
    : normalized.slice(4, closingDelimiter);
  return optionalField("name", frontmatter).name;
}

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
