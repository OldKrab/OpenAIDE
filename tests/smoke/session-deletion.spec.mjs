import { readFile } from "node:fs/promises";
import path from "node:path";
import { expect, test } from "@playwright/test";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);
test.use({ actionTimeout: 10_000 });
let harness;
test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(180_000);
  harness = await startFullStackHarness();
});
test.afterAll(async () => { await harness?.close(); });
test.afterEach(async ({}, testInfo) => {
  if (testInfo.status !== testInfo.expectedStatus) {
    const diagnostics = await readFile(path.join(harness.stateRoot, "diagnostics/logs/openaide-app-server.jsonl")).catch(() => Buffer.from("Diagnostics unavailable"));
    await testInfo.attach("app-server.jsonl", { body: diagnostics, contentType: "text/plain" });
    await testInfo.attach("full-stack.log", { body: Buffer.from(harness.logs.join("\n")), contentType: "text/plain" });
  }
});

test("Archive explains local retention and Delete removes archived history from both viewing clients", async ({ page, browser }, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 850 });
  await prepare(page);
  await send(page, "smoke:basic");
  await expect(page.getByLabel("Task status: Idle")).toBeVisible();
  const taskUrl = page.url();
  await openMenu(page, "Smoke task");
  await page.getByRole("menuitem", { name: "Archive task", exact: true }).click();
  const archive = page.getByRole("dialog", { name: "Archive task?" });
  await expect(archive).toContainText("Its Agent history stays available");
  await archive.getByRole("button", { name: "Archive task", exact: true }).click();
  await expect(page).toHaveURL(/\/new-task/);
  await page.getByRole("button", { name: "Archive", exact: true }).click();
  const row = page.getByRole("listitem").filter({ hasText: "Smoke task" }).first();
  await row.locator(".task-open").click();
  await expect(page).toHaveURL(taskUrl);

  const secondContext = await browser.newContext({ viewport: { width: 1280, height: 850 } });
  const second = await secondContext.newPage();
  await second.goto(taskUrl);
  await expect(second.getByLabel("Task chat")).toBeVisible();
  await openMenu(page, "Smoke task");
  await page.getByRole("menuitem", { name: "Delete session…" }).click();
  const dialog = page.getByRole("dialog", { name: "Delete session?" });
  await expect(dialog).toContainText("No queued messages.");
  await expect(dialog).toContainText("This does not promise permanent erasure");
  await page.screenshot({ path: testInfo.outputPath("delete-archived-wide.png") });
  await dialog.getByRole("button", { name: "Delete session", exact: true }).click();
  await expect(page).toHaveURL(/\/new-task/);
  await expect(second).toHaveURL(/\/new-task/);
  await expect(page.getByRole("listitem").filter({ hasText: "Smoke task" })).toHaveCount(0);
  await secondContext.close();
});

test("narrow navigation requires the extra active-work confirmation", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await prepare(page);
  await send(page, "smoke:hold");
  await expect(page.getByText("Waiting for steering", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Open task navigation", exact: true }).click();
  await openMenu(page, "smoke:hold");
  await page.getByRole("menuitem", { name: "Delete session…" }).click();
  const ordinary = page.getByRole("dialog", { name: "Delete session?" });
  await ordinary.getByRole("button", { name: "Continue", exact: true }).click();
  const active = page.getByRole("dialog", { name: "Delete while work is active?" });
  await expect(active).toContainText("If the Agent refuses, the task stays available");
  const geometry = await active.evaluate((element) => ({ width: element.getBoundingClientRect().width, scrollWidth: element.scrollWidth, clientWidth: element.clientWidth }));
  expect(geometry.width).toBeLessThanOrEqual(390);
  expect(geometry.scrollWidth).toBe(geometry.clientWidth);
  await page.screenshot({ path: testInfo.outputPath("delete-active-narrow.png") });
  await active.getByRole("button", { name: "Delete session", exact: true }).click();
  await expect(page).toHaveURL(/\/new-task/);
});

async function prepare(page) {
  await page.goto(`${harness.baseUrl}/new-task`);
  const context = page.getByLabel("Task start context");
  const agent = context.locator(".new-task-context-anchor-agent > button");
  await expect(agent).toBeVisible();
  if ((await agent.textContent())?.trim() !== "OpenAIDE Test Agent") {
    await agent.click();
    await page.getByRole("menu", { name: "Agent" }).getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ }).click();
  }
  await expect(page.getByRole("button", { name: "Balanced", exact: true })).toBeEnabled();
}
async function send(page, text) {
  await page.getByRole("textbox", { name: "Message" }).fill(text);
  await page.getByLabel("Send message").click();
  await expect(page).toHaveURL(/\/task\/task_/);
}
async function openMenu(page, title) {
  const row = page.getByRole("listitem").filter({ hasText: title }).first();
  await row.hover();
  await row.getByRole("button", { name: `Task actions for ${title}`, exact: true }).click();
}
