import { expect, test } from "@playwright/test";
import { mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.use({ actionTimeout: 10_000 });
test.setTimeout(120_000);

let harness;
const shots = path.join("test-results", `file-viewer-layout-${process.env.OPENAIDE_SMOKE_FRONTEND ?? "web"}`);

test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(120_000);
  harness = await startFullStackHarness({ frontend: process.env.OPENAIDE_SMOKE_FRONTEND ?? "web" });
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

  await expect(page.getByRole("button", { name: "Back to conversation" })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Project navigation", exact: true })).toBeVisible();
  await page.screenshot({ path: path.join(shots, "wide-file-open.png") });
  const explorer = page.locator(".project-files-list");
  await expect(explorer.getByText("Loading files…", { exact: true })).toHaveCount(0);
  const scrollBefore = await explorer.evaluate(el => { el.scrollTop = 500; return el.scrollTop; });
  expect(scrollBefore).toBeGreaterThan(100);
  await page.getByRole("button", { name: "Back to conversation" }).click();
  await page.evaluate(() => {
    window.explorerScrollSamples = [];
    let frames = 0;
    const sample = () => {
      const list = document.querySelector(".project-files-list");
      if (list?.getClientRects().length) window.explorerScrollSamples.push(list.scrollTop);
      if (++frames < 90) requestAnimationFrame(sample);
      else window.explorerScrollFinished = true;
    };
    requestAnimationFrame(sample);
  });
  await page.getByRole("button", { name: "Project files", exact: true }).click();
  await page.waitForFunction(() => window.explorerScrollFinished);
  const samples = await page.evaluate(() => window.explorerScrollSamples);
  expect(samples.length).toBeGreaterThan(0);
  expect(Math.max(...samples) - Math.min(...samples), "Reopening Files moved the explorer during refresh").toBeLessThanOrEqual(1);
  expect(samples.at(-1)).toBe(scrollBefore);

  await page.getByRole("button", { name: "Back to conversation" }).click();
  await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await readme.click();
  await expect(fileViewer).toBeVisible();
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
  await fileViewer.getByRole("button", { name: "Select line 1", exact: true }).click();
  await fileViewer.getByRole("button", { name: "Comment", exact: true }).click();
  const composer = page.getByRole("textbox", { name: "Message" });
  await expect(composer).toBeVisible();
  await expect(composer).toContainText("README.md:1");
  await page.screenshot({ path: path.join(shots, "phone-back-to-chat.png") });

  const phonePlan = page.locator("aside.task-plan-drawer[data-open='true']");
  if (!await phonePlan.isVisible()) await page.getByRole("button", { name: "Open Plan", exact: true }).click();
  await expect(phonePlan).toBeVisible();
  await page.screenshot({ path: path.join(shots, "phone-plan-open.png") });
  const gap = await page.evaluate(() => {
    const bar = document.querySelector(".mobile-workbench-bar")?.getBoundingClientRect();
    const drawer = document.querySelector("aside.task-plan-drawer[data-open='true']")?.getBoundingClientRect();
    const workbench = document.querySelector(".task-workbench")?.getBoundingClientRect();
    return { bar, drawer, workbench };
  });
  const barBottom = gap.bar.y + gap.bar.height;
  expect(gap.workbench.y - barBottom).toBeLessThan(48);
  expect(gap.workbench.width).toBeGreaterThan(380);
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
    if (await planTrigger.isVisible() && await planTrigger.getAttribute("aria-expanded") === "true") await planTrigger.click();
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
    if (await plan.isVisible() && await plan.getAttribute("aria-expanded") === "true") await plan.click();
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

test("browses independent files, searches exact lines, and comments beside source/diff selections", async ({ page }) => {
  const filename = "000-project-access-smoke.ts";
  await writeFile(filename, "export const first = 1;\nexport const second = 2;\nexport const projectAccessNeedle = 3;\n");
  try {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${harness.baseUrl}/new-task?desktop-platform=${process.env.OPENAIDE_SMOKE_DESKTOP_PLATFORM ?? "macos"}`);
    await page.getByRole("textbox", { name: "Message" }).fill(`smoke:file-viewer-layout\ndownload-file:${filename}`);
    await page.getByLabel("Send message").click();
    await expect(page.getByLabel("Task chat").getByRole("link", { name: "README.md" })).toBeVisible();
    await page.getByRole("textbox", { name: "Message" }).fill("Keep this draft");
    await page.getByRole("button", { name: "Project files", exact: true }).click();
    const project = page.getByRole("region", { name: "Project files", exact: true });
    await project.getByRole("treeitem", { name: filename, exact: true }).click();
    await project.getByRole("textbox", { name: "Find a file" }).fill(filename);
    await project.locator(".project-file-result").filter({ hasText: filename }).click();
    const reader = project.locator(".file-code-reader");
    await expect(reader.getByRole("button", { name: "Select line 3", exact: true })).toBeVisible();
    const first = await reader.getByRole("button", { name: "Select line 1", exact: true }).boundingBox();
    const third = await reader.getByRole("button", { name: "Select line 3", exact: true }).boundingBox();
    await page.mouse.move(first.x + first.width / 2, first.y + first.height / 2);
    await page.mouse.down();
    await page.mouse.move(third.x + third.width / 2, third.y + third.height / 2, { steps: 6 });
    await expect(reader.getByRole("button", { name: "Comment", exact: true })).toBeHidden();
    await page.mouse.up();
    await expect(reader.locator('[data-selected="true"]')).toHaveCount(3);
    await expect(reader.getByRole("button", { name: "Select range", exact: true })).toBeHidden();
    const action = reader.getByRole("button", { name: "Comment", exact: true });
    await expect(action).toBeVisible();
    const actionBox = await action.boundingBox();
    expect(actionBox.y - third.y).toBeLessThan(65);
    await page.screenshot({ path: path.join(shots, "project-source-selection.png") });
    await action.click();
    const composer = page.getByRole("textbox", { name: "Message" });
    await expect(composer).toContainText("Keep this draft");
    await expect(composer).toContainText(`${filename}:1–3`);
    await page.getByRole("button", { name: "Project files", exact: true }).click();
    await project.getByRole("button", { name: "Search", exact: true }).click();
    await project.getByRole("textbox", { name: "Search file contents" }).fill("projectAccessNeedle");
    await project.locator(".project-file-result").filter({ hasText: filename }).click();
    await expect(reader.locator('[data-focus="true"]')).toContainText("projectAccessNeedle");
    // Native drag selects characters without turning the source into whole-line selection.
    const code = reader.locator(".file-viewer-code").first();
    const bounds = await code.boundingBox();
    await page.mouse.move(bounds.x + 2, bounds.y + bounds.height / 2);
    await page.mouse.down();
    await page.mouse.move(bounds.x + 83, bounds.y + bounds.height / 2, { steps: 8 });
    await page.mouse.up();
    const selected = await page.evaluate(() => window.getSelection().toString());
    expect(selected.length).toBeGreaterThan(0);
    await expect(reader.locator('[data-selected="true"]')).toHaveCount(0);
    await action.click();
    await expect(composer).toContainText(selected);
    await page.getByRole("button", { name: "Project files", exact: true }).click();
    await project.getByRole("button", { name: "Changes", exact: true }).click();
    await project.locator(".project-file-result").filter({ hasText: filename }).click();
    await expect(reader).toHaveAttribute("data-diff", "true");
    await reader.getByRole("button", { name: "Select working copy line 2", exact: true }).click();
    await expect(action).toBeVisible();
    await page.screenshot({ path: path.join(shots, "project-diff-selection.png") });
    await action.click();
    await expect(composer).toContainText("working copy lines 2");
    await page.getByRole("button", { name: "Project files", exact: true }).click();
    await expect(project.getByRole("button", { name: "Changes", exact: true })).toHaveAttribute("aria-pressed", "true");
    await project.getByRole("button", { name: "File", exact: true }).click();
    await expect(reader).toHaveAttribute("data-diff", "false");
    await project.getByRole("button", { name: "Diff", exact: true }).click();
    await expect(reader).toHaveAttribute("data-diff", "true");
    await project.getByRole("button", { name: "Back to conversation" }).click();
    await page.getByLabel("Task chat").getByRole("link", { name: "Build", exact: true }).click();
    await expect(reader).toHaveAttribute("data-diff", "false");
    await page.emulateMedia({ colorScheme: "dark" });
    await page.screenshot({ path: path.join(shots, "project-dark.png") });
    if (process.env.OPENAIDE_SMOKE_FRONTEND === "desktop") {
      // Native windows enforce 880×600; verify the supported minimum, not a phone-sized native window.
      await page.setViewportSize({ width: 880, height: 600 });
      await expect(project.getByRole("button", { name: "Back to conversation" })).toBeVisible();
      await expect(reader).toBeVisible();
      await page.screenshot({ path: path.join(shots, "project-desktop-minimum.png") });
      return;
    }
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(project.getByRole("complementary", { name: "Project navigation", exact: true })).toBeHidden();
    await project.locator(".project-files-mobile-back").click();
    await expect(project.getByRole("complementary", { name: "Project navigation", exact: true })).toBeVisible();
    await page.screenshot({ path: path.join(shots, "project-phone-list.png") });
  } finally { await rm(filename, { force: true }); }
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
