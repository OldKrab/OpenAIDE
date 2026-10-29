import { expect, test } from "@playwright/test";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.setTimeout(120_000);
let harness;

test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(120_000);
  // No rollout environment variable: the shared App Server must negotiate this
  // capability on ordinary connections, including custom ACP Agents.
  harness = await startFullStackHarness({
    agentArgs: ["--native-subagents"],
  });
});
test.afterAll(async () => { await harness?.close(); });

for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
  test(`inspects separate nested Subagent histories after reload at ${viewport.width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await page.goto(`${harness.baseUrl}/new-task`);
    const context = page.getByLabel("Task start context");
    const agent = context.locator(".new-task-context-anchor-agent > button");
    if ((await agent.textContent())?.trim() !== "OpenAIDE Test Agent") {
      await agent.click();
      await page.getByRole("menu", { name: "Agent" })
        .getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ }).click({ force: true });
    }
    const mode = page.getByRole("button", { name: /^(Balanced|Verbose)$/ });
    await expect(mode).toBeEnabled();
    await page.getByRole("textbox", { name: "Message" }).fill("smoke:native-subagents");
    await page.getByLabel("Send message").click();
    const chat = page.getByLabel("Task chat");
    await expect(chat.getByText("Main review result", { exact: true })).toBeVisible();
    await openMoreOnPhone(page);
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Main Agent" })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toHaveCount(0);

    await selectHistory(page, "Review the implementation");
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await expect(chat.getByText("Main review result", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("textbox", { name: "Message" })).toHaveCount(0);
    const details = page.getByRole("button", { name: "Subagent details: Agent type reviewer, Model test-model" });
    await details.click();
    const detailsPanel = page.getByRole("dialog", { name: "Subagent details" });
    await expect(detailsPanel.getByText("Agent type", { exact: true })).toBeVisible();
    await expect(detailsPanel.getByText("test-model", { exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("child-history.png") });
    await page.keyboard.press("Escape");
    // Rehosting the same selector in compact chrome must preserve inspection.
    await page.setViewportSize(viewport.width > 760 ? { width: 390, height: 844 } : { width: 1440, height: 900 });
    await openMoreOnPhone(page);
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Review the implementation" })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await page.setViewportSize(viewport);
    await selectHistory(page, "Check a nested detail");
    await expect(chat.getByText("Nested review result", { exact: true })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toHaveCount(0);
    // A label longer than the bar truncates; it must not widen Chat past the viewport.
    const returnBox = await page.getByRole("button", { name: "Back to Main Agent" }).boundingBox();
    expect(returnBox.x + returnBox.width).toBeLessThanOrEqual(viewport.width);
    const nestedBox = await chat.getByText("Nested review result", { exact: true }).boundingBox();
    expect(nestedBox.x + nestedBox.width).toBeLessThanOrEqual(viewport.width);
    // A Subagent whose Agent reported no details shows no empty disclosure.
    await expect(page.getByRole("button", { name: /^Subagent details/ })).toHaveCount(0);
    await openMoreOnPhone(page);
    await page.getByRole("button", { name: /^Switch agent\. Currently viewing/ }).click();
    await expect(page.getByRole("menuitemradio", { name: /^Main Agent/ })).toBeVisible();
    // A delegated task longer than the menu truncates; its status still fits in the row.
    const longRow = page.getByRole("menuitemradio", { name: /^Check a nested detail/ });
    const menuBox = await page.getByRole("menu", { name: "Switch agent" }).boundingBox();
    const rowBox = await longRow.boundingBox();
    const statusBox = await longRow.getByText("Completed", { exact: true }).boundingBox();
    expect(rowBox.x + rowBox.width).toBeLessThanOrEqual(menuBox.x + menuBox.width + 0.5);
    expect(statusBox.x + statusBox.width).toBeLessThanOrEqual(rowBox.x + rowBox.width);
    await page.screenshot({ path: testInfo.outputPath("history-selector.png") });
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: "Back to Main Agent" }).click();
    await expect(chat.getByText("Main review result", { exact: true })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
    await selectHistory(page, "Review the implementation");
    await page.reload();
    await openMoreOnPhone(page);
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Main Agent" })).toBeVisible();
    await expect(chat.getByText("Main review result", { exact: true })).toBeVisible();
    await selectHistory(page, "Review the implementation");
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await selectHistory(page, "Check a nested detail");
    await expect(chat.getByText("Nested review result", { exact: true })).toBeVisible();
  });
}

test("follows released-form Subagents and stops a running one", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${harness.baseUrl}/new-task`);
  const context = page.getByLabel("Task start context");
  const agent = context.locator(".new-task-context-anchor-agent > button");
  if ((await agent.textContent())?.trim() !== "OpenAIDE Test Agent") {
    await agent.click();
    await page.getByRole("menu", { name: "Agent" })
      .getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ }).click({ force: true });
  }
  await expect(page.getByRole("button", { name: /^(Balanced|Verbose)$/ })).toBeEnabled();
  await page.getByRole("textbox", { name: "Message" }).fill("smoke:released-subagents");
  await page.getByLabel("Send message").click();
  const chat = page.getByLabel("Task chat");
  await expect(chat.getByText("Main delegation result", { exact: true })).toBeVisible();
  // The Agent's own Tool rows show the delegation; no lifecycle row is added.
  await expect(chat.getByText(/Delegated to/)).toHaveCount(0);

  await selectHistory(page, "Survey the code");
  await expect(chat.getByText("Survey the code please", { exact: true })).toBeVisible();
  await expect(chat.getByText("Survey result", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Stop Subagent/ })).toHaveCount(0);

  await selectHistory(page, "Explore the history");
  await expect(chat.getByText("Explore the history please", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Stop Subagent: Explore the history" }).click();
  await expect(page.getByRole("button", { name: /^Stop Subagent/ })).toHaveCount(0);
  await page.getByRole("button", { name: /^Switch agent\. Currently viewing/ }).click();
  // Each row leads with the task the Agent delegated and keeps the Agent's name beneath it.
  await expect(page.getByRole("menuitemradio", { name: /^Explore the history\b.*Cancelled.*\bExplorer$/ })).toBeVisible();
  await expect(page.getByRole("menuitemradio", { name: /^Survey the code\b.*Completed.*\bResearcher$/ })).toBeVisible();
  await page.keyboard.press("Escape");
});

/** Below this width the Task bar is compact (`mobile-chat.css`). */
const COMPACT_TASK_BAR_WIDTH = 760;

/**
 * On a phone the Agent switcher waits in the Task bar's More row, which stays open
 * across resizes. The row is opened only when it is closed: role names match as
 * substrings, so "More Task actions" also finds the open row's "Hide more Task
 * actions", and clicking it closed the row. The layout follows the viewport rather
 * than a visibility check made while a resize may still be settling.
 */
async function openMoreOnPhone(page) {
  const switcher = page.getByRole("button", { name: /^Switch agent\. Currently viewing/ });
  if (page.viewportSize().width > COMPACT_TASK_BAR_WIDTH) {
    await expect(switcher).toBeVisible();
    return;
  }
  const more = page.getByRole("button", { name: "More Task actions", exact: true });
  const hide = page.getByRole("button", { name: "Hide more Task actions", exact: true });
  await expect(more.or(hide).first()).toBeVisible();
  if (await more.isVisible()) await more.click();
  await expect(switcher).toBeVisible();
}

async function selectHistory(page, name) {
  await openMoreOnPhone(page);
  await page.getByRole("button", { name: /^Switch agent\. Currently viewing/ }).click();
  await page.getByRole("menuitemradio", { name: new RegExp(`^${name}\\b`) }).click();
}
