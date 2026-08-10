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
