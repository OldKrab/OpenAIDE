import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { SettingsTabId } from "@openaide/app-shell-contracts";

/** Background a curious user can ask for; never shown until its icon is pressed. */
export type InfoText = { title: string; body: string[] };

export type SettingsTone = "ok" | "warn" | "off" | "busy";

export type SettingsRow = {
  id: string;
  label: string;
  /** One short line that adds a fact the label does not carry. */
  description?: string;
  /** Extra words the search matches; never shown. */
  keywords?: string;
  leading?: ReactNode;
  /** Background for this one setting; section-wide background goes on the group or section. */
  info?: InfoText;
  status?: { tone: SettingsTone; text: string };
  control?: ReactNode;
  /** Full-width content under the label, for settings that are not a single control. */
  below?: ReactNode;
  /** A failure of this row's own action, shown at the row. */
  error?: string;
  selected?: boolean;
  onOpen?: () => void;
};

export type SettingsGroup = {
  id: string;
  title?: string;
  info?: InfoText;
  actions?: ReactNode;
  /** Shown above the rows: a failure or a state that concerns the whole group. */
  notice?: ReactNode;
  /** Shown instead of the card when the group has no rows. */
  empty?: string;
  rows: SettingsRow[];
};

/**
 * One Settings section, declared once. The same declaration renders the page,
 * feeds the search, and places the info affordances, so a setting cannot be
 * visible on the page and missing from search.
 */
export type SettingsSection = {
  id: SettingsTabId;
  label: string;
  icon: LucideIcon;
  /** Current choice worth seeing from the navigation, such as the computer in use. */
  value?: string;
  info?: InfoText;
  action?: ReactNode;
  /** Loading, failure, or empty state for the whole section. */
  notice?: ReactNode;
  groups: SettingsGroup[];
  /** The drilled-in page of one item; replaces the section page while present. */
  detail?: ReactNode;
  /** Dialogs owned by the section; rendered while the section is on screen. */
  overlay?: ReactNode;
  /** Leaves the drilled-in page. Returns false when the section kept it open to ask first. */
  closeDetail?: () => boolean;
};

export function normalizeNeedle(query: string) {
  return query.trim().toLowerCase();
}

export function infoMatches(info: InfoText | undefined, needle: string) {
  return Boolean(needle && info && infoText(info).includes(needle));
}

/**
 * Keeps the Settings structure and drops what does not match, the way an IDE
 * settings search does. A matching section or group keeps all of its rows.
 */
export function filterSections(sections: SettingsSection[], needle: string): SettingsSection[] {
  if (!needle) return sections;
  return sections.flatMap((section) => {
    if (`${section.label} ${infoText(section.info)}`.toLowerCase().includes(needle)) return [section];
    const groups = section.groups.flatMap((group) => {
      if (groupText(group).includes(needle)) return [group];
      const rows = group.rows.filter((row) => rowText(row).includes(needle));
      return rows.length ? [{ ...group, rows }] : [];
    });
    return groups.length ? [{ ...section, groups }] : [];
  });
}

function infoText(info?: InfoText) {
  return info ? `${info.title} ${info.body.join(" ")}`.toLowerCase() : "";
}

function groupText(group: SettingsGroup) {
  return `${group.title ?? ""} ${infoText(group.info)}`.toLowerCase();
}

function rowText(row: SettingsRow) {
  return `${row.label} ${row.keywords ?? ""} ${row.description ?? ""} ${row.status?.text ?? ""} ${infoText(row.info)}`.toLowerCase();
}
