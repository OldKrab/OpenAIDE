import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.setTimeout(120_000);

let harness;
const shots = path.join("test-results", "file-viewer-layout");

test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(120_000);
  harness = await startFullStackHarness();
  await mkdir(shots, { recursive: true });
});

test.afterAll(async ({}, testInfo) => {
  testInfo.setTimeout(30_000);
  await harness?.close();
});

test("keeps Plan on Chat and returns from File Viewer on a phone", async ({ page }) => {
  await page.setViewportSize({ width: 1_280, height: 800 });
  await page.goto(`${harness.baseUrl}/new-task`);
  await expect(page.getByLabel("New task")).toBeVisible();
  const context = page.getByLabel("Task start context");
  const currentAgent = context.locator(".new-task-context-anchor-agent > button");
  if ((await currentAgent.textContent())?.trim() !== "OpenAIDE Test Agent") {
    await currentAgent.click();
    await page.getByRole("menu", { name: "Agent" })
      .getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ })
      .click({ force: true });
  }
  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill("smoke:file-viewer-layout");
  await page.getByLabel("Send message").click();

  const chat = page.getByLabel("Task chat");
  await expect(page).toHaveURL(/\/task\/task_/);
  const readme = chat.getByRole("link", { name: "README.md" });
  await expect(readme).toBeVisible();
  await readme.click();

  const fileViewer = page.getByRole("complementary", { name: "File Viewer" });
  await expect(fileViewer).toBeVisible();
  await expect(fileViewer.getByRole("button", { name: "Back to Chat" })).toBeVisible();
  await expect(fileViewer.getByRole("button", { name: "Back to Chat" })).toContainText("Chat");

  const planChip = page.locator(".task-plan-drawer-trigger");
  await expect(planChip).toBeVisible();
  const planBox = await planChip.boundingBox();
  const fileBox = await fileViewer.boundingBox();
  expect(planBox).toBeTruthy();
  expect(fileBox).toBeTruthy();
  expect(planBox.x + planBox.width).toBeLessThanOrEqual(fileBox.x + 8);

  if (await planChip.getAttribute("aria-expanded") !== "true") {
    await planChip.click();
  }
  await expect(page.locator("aside.task-plan-drawer[data-open='true']")).toBeVisible();
  await page.screenshot({ path: path.join(shots, "wide-plan-open.png") });
  await page.screenshot({ path: path.join(shots, "wide-file-open.png") });
  await planChip.click();

  await fileViewer.getByRole("button", { name: "Back to Chat" }).click();
  await expect(page.getByRole("button", { name: "Show Task Panel" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
  await page.screenshot({ path: path.join(shots, "wide-file-collapsed.png") });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("link", { name: "README.md" }).click();
  await expect(fileViewer.getByRole("button", { name: "Back to Chat" })).toBeVisible();
  await expect(fileViewer.getByRole("button", { name: "Back to Chat" })).toContainText("Chat");
  const tabStrip = await fileViewer.locator(".file-viewer-tabs").evaluate((el) => ({
    verticalBar: el.offsetWidth - el.clientWidth,
    horizontalBar: el.offsetHeight - el.clientHeight,
  }));
  expect(tabStrip.verticalBar, "File Tabs showed a vertical scrollbar").toBe(0);
  expect(tabStrip.horizontalBar, "File Tabs showed a scrollbar under the tabs").toBe(0);
  const tab = fileViewer.getByRole("tab", { name: "README.md" });
  const preview = fileViewer.getByRole("button", { name: "Show Markdown preview" });
  await expect(tab).toBeVisible();
  await expect(preview).toBeVisible();
  const tabBox = await tab.boundingBox();
  const previewBox = await preview.boundingBox();
  expect(tabBox && previewBox).toBeTruthy();
  const overlap = tabBox.x < previewBox.x + previewBox.width
    && previewBox.x < tabBox.x + tabBox.width
    && tabBox.y < previewBox.y + previewBox.height
    && previewBox.y < tabBox.y + tabBox.height;
  expect(overlap, "Preview overlapped the File Tab").toBe(false);
  await page.screenshot({ path: path.join(shots, "phone-file-open.png") });

  await fileViewer.getByRole("button", { name: "Show raw Markdown" }).click();
  await fileViewer.getByRole("button", { name: "Quote line 1", exact: true }).click();
  const composer = page.getByRole("textbox", { name: "Message" });
  await expect(composer).toBeVisible();
  await expect(composer).toContainText("README.md:1");
  await page.screenshot({ path: path.join(shots, "phone-back-to-chat.png") });

  await page.getByRole("button", { name: "Open Plan" }).click();
  const phonePlan = page.locator("aside.task-plan-drawer[data-open='true']");
  await expect(phonePlan).toBeVisible();
  await page.screenshot({ path: path.join(shots, "phone-plan-open.png") });
  const gap = await page.evaluate(() => {
    const bar = document.querySelector(".mobile-workbench-bar")?.getBoundingClientRect();
    const drawer = document.querySelector("aside.task-plan-drawer[data-open='true']")?.getBoundingClientRect();
    const workbench = document.querySelector(".task-workbench")?.getBoundingClientRect();
    return { bar, drawer, workbench };
  });
  const barBottom = gap.bar.y + gap.bar.height;
  expect(gap.workbench.y - barBottom, `Chat started ${gap.workbench.y - barBottom}px below the bar`).toBeLessThan(8);
  expect(gap.drawer.y - barBottom, `Plan started ${gap.drawer.y - barBottom}px below the bar`).toBeLessThan(16);
});
