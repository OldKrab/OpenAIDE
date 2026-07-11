import { describe, expect, it } from "vitest";
import {
  advanceLiveText,
  receiveLiveText,
  stableMarkdownTarget,
  startLiveText,
} from "./liveTextPresentation";

describe("live text presentation", () => {
  it("paces a large received burst instead of draining it within six frames", () => {
    let state = startLiveText("Agent");
    const burst = `Agent ${"streamed words ".repeat(24)}`.trim();

    state = receiveLiveText(state, burst);

    expect(state.visibleText).toBe("Agent");
    expect(state.caughtUp).toBe(false);

    state = advanceLiveText(state);
    expect(state.visibleText.length).toBeGreaterThan("Agent".length);
    expect(state.visibleText).not.toBe(state.receivedText);

    for (let frame = 0; frame < 5; frame += 1) {
      state = advanceLiveText(state);
    }
    expect(state.visibleText).not.toBe(state.receivedText);

    for (let frame = 0; frame < 80 && !state.caughtUp; frame += 1) {
      state = advanceLiveText(state);
    }
    expect(state.visibleText).toBe(state.receivedText);
    expect(state.caughtUp).toBe(true);
  });

  it("holds only an unfinished Markdown suffix until its line is complete", () => {
    expect(stableMarkdownTarget("Use **important")).toBe("Use ");
    expect(stableMarkdownTarget("Use **important** now")).toBe("Use **important** now");
    expect(stableMarkdownTarget("Ordinary text keeps flowing")).toBe("Ordinary text keeps flowing");
    expect(stableMarkdownTarget("Use **important\n")).toBe("Use **important\n");
  });

  it("uses one-line lookahead before presenting a possible GFM table", () => {
    expect(stableMarkdownTarget("Before\n| Name | State")).toBe("Before\n");
    expect(stableMarkdownTarget("Before\n| Name | State |\n")).toBe("Before\n");
    expect(stableMarkdownTarget("Before\n| Name | State |\n| --- | --- |\n")).toBe(
      "Before\n| Name | State |\n| --- | --- |\n",
    );
    expect(stableMarkdownTarget("| Name | State |\n| --- | --- |\n| Build | Passing |\n")).toBe(
      "| Name | State |\n| --- | --- |\n| Build | Passing |\n",
    );
    expect(stableMarkdownTarget("Before\na | normal prose\nNext line\n")).toBe(
      "Before\na | normal prose\nNext line\n",
    );
  });
});
