import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import { BackgroundCommandsRow } from "./BackgroundCommandsRow";

describe("BackgroundCommandsRow", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it.each([
    [0, "Background command finished"],
    [1, "Subagent running"],
    [2, "2 subagents running"],
  ])("names what holds a turn with no live command (%i running subagents)", (runningSubagentCount, label) => {
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<BackgroundCommandsRow commands={[]} runningSubagentCount={runningSubagentCount} />);
    });
    expect(tree.root.findByProps({ role: "status" }).children.join("")).toBe(label);
  });
});
