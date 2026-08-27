import { expect, test } from "@playwright/test";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.setTimeout(120_000);

let harness;

test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(120_000);
  harness = await startFullStackHarness({
    agentArgs: ["--active-writer"],
    frontend: "desktop",
  });
});

test.afterAll(async ({}, testInfo) => {
  testInfo.setTimeout(30_000);
  await harness?.close();
});

test.afterEach(async ({}, testInfo) => {
  if (testInfo.status !== testInfo.expectedStatus && harness?.logs.length) {
    await testInfo.attach("full-stack.log", {
      body: Buffer.from(harness.logs.join("\n")),
      contentType: "text/plain",
    });
  }
});

test("keeps Desktop New Task controls in the workspace without a title strip", async ({ page }) => {
  await page.setViewportSize({ width: 1_440, height: 900 });
  await page.goto(`${harness.baseUrl}/new-task`);

  await expect(page.getByLabel("New task")).toBeVisible();
  await expect(page.locator(".desktop-title-bar-macos")).toBeVisible();
  await expect(page.locator(".app-sidebar-frame-header")).toHaveCount(0);
  await expect(page.locator(".task-header")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Balanced", exact: true })).toBeVisible();

  await page.setViewportSize({ width: 900, height: 640 });
  await expect(page.getByLabel("New task")).toBeVisible();
  await expect(page.locator(".app-sidebar-frame-header")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Balanced", exact: true })).toBeVisible();
});

test("merges an active Task header into Windows chrome", async ({ page }) => {
  await page.setViewportSize({ width: 993, height: 640 });
  await page.goto(`${harness.baseUrl}/new-task?desktop-platform=windows`);

  const context = page.getByLabel("Task start context");
  const currentAgent = context.locator(".new-task-context-anchor-agent > button");
  if ((await currentAgent.textContent())?.trim() !== "OpenAIDE Test Agent") {
    await currentAgent.click();
    await page.getByRole("menu", { name: "Agent" })
      .getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ })
      .click({ force: true });
  }
  await page.getByRole("textbox", { name: "Message" }).fill("smoke:basic");
  await page.getByLabel("Send message").click();

  await expect(page.getByLabel("Task chat")).toBeVisible();
  await expect(page.locator(".task-header-title > strong")).toHaveText("Smoke task");
  await expect(page.locator(".desktop-title-bar-integrated")).toBeVisible();
  await expect(page.locator(".desktop-title-bar-label")).toHaveCount(0);
  await expect(page.getByLabel("Window controls", { exact: true })).toBeVisible();

  const layout = await page.evaluate(() => {
    const chrome = document.querySelector(".desktop-title-bar-integrated");
    const taskHeader = document.querySelector(".task-work-stack-header");
    const actions = document.querySelector(".task-work-stack-header-actions");
    const controls = document.querySelector(".desktop-caption-buttons");
    if (!chrome || !taskHeader || !actions || !controls) throw new Error("Merged Desktop Task header is incomplete.");
    const chromeRect = chrome.getBoundingClientRect();
    const taskRect = taskHeader.getBoundingClientRect();
    const controlsRect = controls.getBoundingClientRect();
    const productControlRight = Math.max(
      ...Array.from(taskHeader.querySelectorAll("button"), (button) => button.getBoundingClientRect().right),
      0,
    );
    return {
      chromeTop: chromeRect.top,
      chromeHeight: chromeRect.height,
      taskTop: taskRect.top,
      taskHeight: taskRect.height,
      productControlRight,
      controlsLeft: controlsRect.left,
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    };
  });
  expect(layout.taskTop).toBeCloseTo(layout.chromeTop, 0);
  expect(layout.taskHeight).toBeCloseTo(layout.chromeHeight, 0);
  expect(layout.productControlRight).toBeLessThanOrEqual(layout.controlsLeft);
  expect(layout.scrollWidth).toBe(layout.clientWidth);

  await page.setViewportSize({ width: 800, height: 640 });
  await expect(page.locator(".task-header-title > strong")).toHaveText("Smoke task");
  await expect(page.getByLabel("Window controls", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth))
    .toBe(await page.evaluate(() => document.documentElement.clientWidth));
});

test("explains when a Native Session is already open in another window", async ({ page }) => {
  await page.setViewportSize({ width: 1_440, height: 900 });
  await page.goto(`${harness.baseUrl}/new-task`);

  await page.getByRole("button", { name: "Refresh tasks" }).click();
  const openExternalSession = page.getByRole("button", {
    name: "Open Session open elsewhere",
    exact: true,
  }).first();
  await expect(openExternalSession).toBeVisible({ timeout: 10_000 });
  await openExternalSession.evaluate((element) => element.click());

  const conflict = page.locator('section[aria-label="Session open elsewhere"]');
  await expect(conflict).toBeVisible();
  await expect(conflict.getByText("Session open elsewhere.", { exact: true })).toBeVisible();
  await expect(conflict.getByText("Close it there, then try again.", { exact: false })).toBeVisible();
  await expect(conflict).not.toContainText("active writer");
  await expect(conflict).not.toContainText("thread smoke-active-writer");
});
