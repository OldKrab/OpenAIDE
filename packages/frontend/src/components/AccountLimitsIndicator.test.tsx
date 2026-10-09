// @vitest-environment jsdom
// timing-file: mocked — the announcement window runs on Vitest fake timers.

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentAccountLimits } from "@openaide/app-server-client";
import { ComposerWithContextUsage } from "./ContextUsageIndicator";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const NOON = Date.parse("2026-10-09T12:00:00");

let container: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
let media: EventTarget & { matches: boolean };

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOON);
  media = Object.assign(new EventTarget(), { matches: false });
  vi.stubGlobal("matchMedia", () => media);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

function limits(fiveHour: number, overrides: Partial<AgentAccountLimits["windows"][number]> = {}): AgentAccountLimits {
  return {
    planLabel: "Max",
    windows: [
      { kind: "fiveHour", usedPercent: fiveHour, resetsAtMs: Date.parse("2026-10-09T14:30:00"), status: "ok", ...overrides },
      { kind: "weekly", usedPercent: 22, resetsAtMs: Date.parse("2026-10-12T09:00:00"), status: "ok" },
      { kind: "weeklyModel", modelLabel: "Opus", usedPercent: 17, resetsAtMs: Date.parse("2026-10-12T09:00:00"), status: "ok" },
    ],
  };
}

async function render(accountLimits?: AgentAccountLimits) {
  await act(async () => root.render(
    <ComposerWithContextUsage accountLimits={accountLimits} agentLabel="Claude">
      <section className="composer" />
    </ComposerWithContextUsage>,
  ));
}

const corner = () => container.querySelector<HTMLButtonElement>(".account-limits-corner");
const label = () => container.querySelector<HTMLElement>(".account-limits-label");

describe("Composer account limits", () => {
  it("draws nothing until the Agent has reported limits", async () => {
    await render();
    expect(corner()).toBeNull();
    expect(label()).toBeNull();
  });

  it("names the 5-hour window and keeps the per-model window out of the corner", async () => {
    await render(limits(38));
    expect(corner()?.getAttribute("aria-label")).toBe("Claude 5-hour limit: 38%. Show limits");
    expect(container.querySelectorAll(".account-limits-ring")).toHaveLength(2);
    expect(label()?.textContent).toContain("5-hour limit 38% · resets at");
    expect(label()?.textContent).toContain("Weekly limit 22% · resets on");
    expect(label()?.textContent).not.toContain("Opus");
  });

  it("leads with the window that is reached", async () => {
    const value = limits(40);
    value.windows[1] = { ...value.windows[1]!, status: "reached", usedPercent: 100 };
    await render(value);
    expect(corner()?.getAttribute("aria-label")).toBe("Claude weekly limit: reached. Show limits");
    expect(corner()?.classList.contains("account-limits-reached")).toBe(true);
  });

  it("opens every window with its reset in the details panel", async () => {
    await render(limits(38));
    await act(async () => corner()!.click());
    const panel = document.querySelector(".account-limits-panel");
    expect(panel?.textContent).toContain("Claude limits");
    expect(panel?.textContent).toContain("Max plan");
    expect(panel?.textContent).toContain("Shared by all tasks");
    expect(panel?.textContent).toContain("38% used");
    expect(panel?.textContent).toContain("in 2 h 30 min");
    expect(panel?.textContent).toContain("Weekly · Opus");
    expect(panel?.textContent).toContain("in 3 days");
  });

  it("announces a crossed 10% step and stays quiet inside one", async () => {
    await render(limits(38));
    expect(label()?.dataset.announce).toBe("false");

    await render(limits(39));
    expect(label()?.dataset.announce).toBe("false");

    await render(limits(42));
    expect(label()?.dataset.announce).toBe("true");

    await act(async () => vi.advanceTimersByTime(3_000));
    expect(label()?.dataset.announce).toBe("false");
  });

  it("draws only the headline window in the compact corner", async () => {
    media.matches = true;
    const value = limits(24);
    value.windows[1] = { ...value.windows[1]!, status: "warning", usedPercent: 91 };
    await render(value);
    expect(corner()?.dataset.compact).toBe("true");
    expect(container.querySelectorAll(".account-limits-ring")).toHaveLength(1);
    expect(container.querySelector(".account-limits-ring")?.classList.contains("account-limits-warning")).toBe(true);
    expect(label()?.children).toHaveLength(1);
    expect(label()?.textContent).toContain("Weekly limit 91%");
  });
});
