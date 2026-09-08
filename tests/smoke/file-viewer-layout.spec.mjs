import { expect, test } from "@playwright/test";
import { mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.use({ actionTimeout: 10_000 });
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
  const downloadEvent = page.waitForEvent("download");
  await fileViewer.getByRole("button", { name: "Download file", exact: true }).click();
  const download = await downloadEvent;
  expect(download.suggestedFilename()).toBe("README.md");
  expect(await readFile(await download.path())).toEqual(await readFile("README.md"));

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


test("downloads current binary bytes through a relative symlink link and retries missing files", async ({ page }) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "openaide-download-test-"));
  const target = path.join(directory, "actual.bin");
  const alias = path.join(directory, "сборка build.vsix");
  try {
    await writeFile(target, Buffer.from([0xff, 0]));
    await symlink(target, alias);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto(`${harness.baseUrl}/new-task`);
    await expect(page.getByLabel("New task")).toBeVisible();
    const currentAgent = page.getByLabel("Task start context").locator(".new-task-context-anchor-agent > button");
    if ((await currentAgent.textContent())?.trim() !== "OpenAIDE Test Agent") {
      await currentAgent.click();
      await page.getByRole("menu", { name: "Agent" }).getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ }).click({ force: true });
    }
    await page.getByRole("textbox", { name: "Message" }).fill(`smoke:file-viewer-layout\ndownload-file:${path.relative(process.cwd(), alias)}`);
    await page.getByLabel("Send message").click();
    await page.getByLabel("Task chat").getByRole("link", { name: "Build", exact: true }).click();
    const viewer = page.getByRole("complementary", { name: "File Viewer" });
    await expect(viewer.getByText("Unsupported file", { exact: true })).toBeVisible();
    const bytes = Buffer.alloc(2 * 1024 * 1024, 0xfd);
    await writeFile(target, bytes);
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      viewer.getByRole("button", { name: "Download file", exact: true }).click(),
    ]);
    expect(download.suggestedFilename()).toBe("сборка build.vsix");
    expect(await readFile(await download.path())).toEqual(bytes);
    await page.screenshot({ path: path.join(shots, "wide-binary-download.png") });

    const planTrigger = page.locator(".task-plan-drawer-trigger");
    if (await planTrigger.getAttribute("aria-expanded") === "true") await planTrigger.click();
    await rename(target, path.join(directory, "moved.bin"));
    await page.setViewportSize({ width: 390, height: 844 });
    await viewer.getByRole("button", { name: "Download file", exact: true }).click();
    await expect(viewer.getByRole("alert")).toContainText("File not found");
    await page.screenshot({ path: path.join(shots, "phone-download-error.png") });
    await writeFile(target, Buffer.from([0xfa, 0]));
    const [retried] = await Promise.all([
      page.waitForEvent("download"),
      viewer.getByRole("button", { name: "Retry download" }).click(),
    ]);
    expect(await readFile(await retried.path())).toEqual(Buffer.from([0xfa, 0]));
    await expect(viewer.getByRole("alert")).toHaveCount(0);
    await expect(viewer.getByText("Unsupported file", { exact: true })).toBeVisible();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("shows a bounded photo preview with zoom and original download", async ({ page }) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "openaide-photo-test-"));
  const filename = "large-photo.jpg";
  const imagePath = path.join(directory, filename);
  try {
    // An optional local source verifies a reported photo without checking personal content into the repo.
    const original = process.env.OPENAIDE_SMOKE_IMAGE
      ? await readFile(process.env.OPENAIDE_SMOKE_IMAGE)
      : await photoFixture(page);
    await writeFile(imagePath, original);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto(`${harness.baseUrl}/new-task`);
    await expect(page.getByLabel("New task")).toBeVisible();
    const agent = page.getByLabel("Task start context").locator(".new-task-context-anchor-agent > button");
    if ((await agent.textContent())?.trim() !== "OpenAIDE Test Agent") {
      await agent.click();
      await page.getByRole("menu", { name: "Agent" }).getByRole("menuitemradio", { name: /OpenAIDE Test Agent/ }).click({ force: true });
    }
    await page.getByRole("textbox", { name: "Message" }).fill(`smoke:file-viewer-layout\ndownload-file:${imagePath}`);
    await page.getByLabel("Send message").click();
    await page.getByLabel("Task chat").getByRole("link", { name: "Build", exact: true }).click();
    const viewer = page.getByRole("complementary", { name: "File Viewer" });
    const image = viewer.getByRole("img", { name: filename, exact: true });
    await expect(image).toBeVisible({ timeout: 30_000 });
    await expect.poll(() => image.evaluate((img) => img.complete && img.naturalWidth > 0)).toBe(true);
    await expect(viewer.getByText("Reduced image preview", { exact: true })).toBeVisible();
    const dimensions = await image.evaluate((img) => ({ width: img.naturalWidth, height: img.naturalHeight }));
    expect(Math.max(dimensions.width, dimensions.height)).toBeLessThanOrEqual(2048);
    expect(await image.evaluate((img) => atob(img.src.split(",")[1]).length)).toBeLessThanOrEqual(2 * 1024 * 1024);
    await viewer.getByRole("button", { name: "Zoom image in", exact: true }).click();
    await expect(viewer.getByRole("button", { name: "Reset image zoom" })).toHaveText("125%");
    await viewer.getByRole("button", { name: "Reset image zoom" }).click();
    const plan = page.locator(".task-plan-drawer-trigger");
    if (await plan.getAttribute("aria-expanded") === "true") await plan.click();
    await page.screenshot({ path: path.join(shots, "wide-photo-preview.png"), animations: "disabled" });
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(viewer.getByText("Reduced image preview", { exact: true })).toBeVisible();
    await expect(image).toBeVisible();
    await page.screenshot({ path: path.join(shots, "phone-photo-preview.png"), animations: "disabled" });
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      viewer.getByRole("button", { name: "Download file", exact: true }).click(),
    ]);
    expect(download.suggestedFilename()).toBe(filename);
    expect(await readFile(await download.path())).toEqual(original);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

async function photoFixture(page) {
  const dataUrl = await page.evaluate(() => {
    const canvas = document.createElement("canvas");
    canvas.width = 3072;
    canvas.height = 4096;
    const context = canvas.getContext("2d");
    const gradient = context.createLinearGradient(0, 0, 3072, 4096);
    gradient.addColorStop(0, "#246a8e");
    gradient.addColorStop(1, "#c5df9c");
    context.fillStyle = gradient;
    context.fillRect(0, 0, 3072, 4096);
    context.fillStyle = "#eed786";
    context.fillRect(768, 1024, 1536, 2048);
    return canvas.toDataURL("image/jpeg", 0.95);
  });
  // Trailing bytes leave a valid JPEG while reproducing the former 5 MiB source rejection.
  return Buffer.concat([Buffer.from(dataUrl.split(",")[1], "base64"), Buffer.alloc(6 * 1024 * 1024)]);
}
