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

const edge = () => container.querySelector<HTMLButtonElement>(".account-limits-edge");
const label = () => container.querySelector<HTMLElement>(".account-limits-label");

describe("Composer account limits", () => {
  it("draws nothing until the Agent has reported limits", async () => {
    await render();
    expect(edge()).toBeNull();
    expect(label()).toBeNull();
  });

  it("names the 5-hour window and keeps the per-model window off the border", async () => {
    await render(limits(38));
    expect(edge()?.getAttribute("aria-label")).toBe("Claude 5-hour limit: 62% left. Show limits");
    expect(container.querySelectorAll(".account-limits-segment")).toHaveLength(2);
    expect(label()?.textContent).toContain("5-hour limit 62% left · resets at");
    expect(label()?.textContent).toContain("Weekly limit 78% left · resets on");
    expect(label()?.textContent).not.toContain("Opus");
  });

  it("leads with the window that is reached", async () => {
    const value = limits(40);
    value.windows[1] = { ...value.windows[1]!, status: "reached", usedPercent: 100 };
    await render(value);
    expect(edge()?.getAttribute("aria-label")).toBe("Claude weekly limit: used up. Show limits");
    expect(edge()?.classList.contains("account-limits-reached")).toBe(true);
  });

  it("opens every window with its reset in the details panel", async () => {
    await render(limits(38));
    await act(async () => edge()!.click());
    const panel = document.querySelector(".account-limits-panel");
    expect(panel?.textContent).toContain("Claude limits");
    expect(panel?.textContent).toContain("Max");
    expect(panel?.textContent).not.toContain("Shared by all tasks");
    expect(panel?.textContent).toContain("62% left");
    expect(panel?.querySelector<HTMLElement>(".account-limits-meter-track span")?.style.width).toBe("62%");
    expect(panel?.textContent).toContain("resets in 2 h 30 min");
    expect(panel?.textContent).toContain("Week · Opus");
    expect(panel?.textContent).toMatch(/resets \w+,? \d/);
  });

  it("colours a window that drains faster than its time runs out", async () => {
    // At noon the 5-hour window is half over, so half of it should still be left.
    await render(limits(52));
    const fiveHour = () => container.querySelector(".account-limits-segment");
    expect(fiveHour()?.classList.contains("account-limits-ok")).toBe(true);

    await render(limits(70));
    expect(fiveHour()?.classList.contains("account-limits-fast")).toBe(true);
    expect(edge()?.classList.contains("account-limits-fast")).toBe(true);
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

  it("keeps both windows on the compact border and names only the headline one", async () => {
    media.matches = true;
    const value = limits(24);
    value.windows[1] = { ...value.windows[1]!, status: "warning", usedPercent: 91 };
    await render(value);
    expect(edge()?.dataset.compact).toBe("true");
    const segments = container.querySelectorAll(".account-limits-segment");
    expect(segments).toHaveLength(2);
    expect(segments[1]?.classList.contains("account-limits-warning")).toBe(true);
    expect(label()?.children).toHaveLength(1);
    expect(label()?.textContent).toContain("Weekly limit 9% left");
  });

  it("closes the context details when the limits open, and the reverse", async () => {
    await act(async () => root.render(
      <ComposerWithContextUsage
        accountLimits={limits(38)}
        agentLabel="Claude"
        usage={{ used_tokens: 40_000, capacity_tokens: 100_000 }}
      >
        <section className="composer" />
      </ComposerWithContextUsage>,
    ));
    const context = () => container.querySelector<HTMLButtonElement>(".context-usage-meter")!;
    const contextPanel = () => document.querySelector(".context-usage-panel-anchor");
    const limitsOpen = () => edge()!.getAttribute("aria-expanded") === "true";

    await act(async () => context().click());
    expect(contextPanel()).not.toBeNull();
    await act(async () => edge()!.click());
    expect(limitsOpen()).toBe(true);
    expect(contextPanel()).toBeNull();
    await act(async () => context().click());
    expect(contextPanel()).not.toBeNull();
    expect(limitsOpen()).toBe(false);
  });
});
