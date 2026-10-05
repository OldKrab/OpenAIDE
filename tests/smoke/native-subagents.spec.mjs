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
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Main Agent" })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toHaveCount(0);

    await selectHistory(page, "Reviewer");
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await expect(chat.getByText("Main review result", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("textbox", { name: "Message" })).toHaveCount(0);
    await page.screenshot({ path: testInfo.outputPath("child-history.png") });
    // Rehosting the same selector in compact chrome must preserve inspection.
    await page.setViewportSize(viewport.width > 760 ? { width: 390, height: 844 } : { width: 1440, height: 900 });
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Reviewer" })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await page.setViewportSize(viewport);
    await selectHistory(page, "Detail reviewer");
    await expect(chat.getByText("Nested review result", { exact: true })).toBeVisible();
    await expect(chat.getByText("Child review result", { exact: true })).toHaveCount(0);
    await page.getByRole("button", { name: /^Switch agent\. Currently viewing/ }).click();
    await expect(page.getByRole("menuitemradio", { name: /^Main Agent/ })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("history-selector.png") });
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: "Back to Main Agent" }).click();
    await expect(chat.getByText("Main review result", { exact: true })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
    await selectHistory(page, "Reviewer");
    await page.reload();
    await expect(page.getByRole("button", { name: "Switch agent. Currently viewing Main Agent" })).toBeVisible();
    await expect(chat.getByText("Main review result", { exact: true })).toBeVisible();
    await selectHistory(page, "Reviewer");
    await expect(chat.getByText("Child review result", { exact: true })).toBeVisible();
    await selectHistory(page, "Detail reviewer");
    await expect(chat.getByText("Nested review result", { exact: true })).toBeVisible();
  });
}

async function selectHistory(page, name) {
  await page.getByRole("button", { name: /^Switch agent\. Currently viewing/ }).click();
  await page.getByRole("menuitemradio", { name: new RegExp(`^${name}\\b`) }).click();
}
