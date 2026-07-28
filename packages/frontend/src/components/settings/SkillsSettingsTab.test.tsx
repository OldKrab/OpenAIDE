import { act, create, type ReactTestInstance, type ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SkillSettingsDetails, SkillSettingsRecord } from "@openaide/app-shell-contracts";

import { SkillsSettingsTab } from "./NonAgentSettingsTabs";

describe("SkillsSettingsTab", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("opens a skill lazily and exposes description, arbitrary fields, instructions, and source", async () => {
    const skill = listedSkill();
    const details: SkillSettingsDetails = {
      generated_at: "now",
      skill,
      document: {
        name: "Research",
        description: "Find primary sources",
        additional_fields: [
          { name: "compatibility", value: "tools:\n- web\n- files" },
        ],
        instructions: "# Workflow\n\nRead every source.",
        source: "---\nname: Research\ndescription: Find primary sources\n---\n# Workflow",
      },
    };
    const onLoadSkill = vi.fn(async () => details);
    let view!: ReactTestRenderer;
    act(() => {
      view = create(
        <SkillsSettingsTab
          availability="available"
          onLoadSkill={onLoadSkill}
          skills={[skill]}
        />,
      );
    });

    await act(async () => {
      buttonByText(view.root, "Research").props.onClick();
    });

    const text = textContent(view.root);
    expect(onLoadSkill).toHaveBeenCalledWith("skill-opaque");
    expect(text).toContain("Description");
    expect(text).toContain("Find primary sources");
    expect(text).toContain("compatibility");
    expect(text).toContain("tools:");
    expect(text).toContain("Workflow");
    expect(text).toContain("View source");
    const back = view.root.findByProps({ "aria-label": "Back to Skills" });
    expect(textContent(back)).toContain("Back to Skills");
  });

  it("searches skills by their visible metadata", () => {
    let view!: ReactTestRenderer;
    act(() => {
      view = create(
        <SkillsSettingsTab
          availability="available"
          onLoadSkill={vi.fn()}
          skills={[
            listedSkill(),
            { ...listedSkill(), id: "skill-review", label: "Code review", description: "Review repository changes" },
          ]}
        />,
      );
    });

    const search = view.root.findByProps({ "aria-label": "Search skills" });
    act(() => search.props.onChange({ currentTarget: { value: "repository" } }));

    expect(textContent(view.root)).toContain("Code review");
    expect(textContent(view.root)).not.toContain("Research");
  });
});

function listedSkill(): SkillSettingsRecord {
  return {
    id: "skill-opaque",
    label: "Research",
    scope: "global",
    source_label: "User configuration",
    status: "valid",
    description: "Find primary sources",
    warnings: [],
    tags: [],
    last_scanned_at: "now",
  };
}

function buttonByText(root: ReactTestInstance, text: string) {
  return root.findAllByType("button").find((button) => textContent(button).includes(text))
    ?? (() => { throw new Error(`Missing button: ${text}`); })();
}

function textContent(node: ReactTestInstance | string): string {
  if (typeof node === "string") return node;
  return node.children.map((child) => typeof child === "string" ? child : textContent(child)).join("");
}
