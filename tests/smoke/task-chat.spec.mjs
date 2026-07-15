import { expect, test } from "@playwright/test";
import { startFullStackHarness } from "./full-stack-harness.mjs";

test.describe.configure({ mode: "serial" });
test.setTimeout(120_000);

let harness;

test.beforeAll(async ({}, testInfo) => {
  testInfo.setTimeout(120_000);
  harness = await startFullStackHarness();
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

test("keeps the New Task form stable across constrained editor heights", async ({ page }) => {
  await page.setViewportSize({ width: 1_000, height: 525 });
  await openPreparedNewTask(page);

  const heading = page.getByRole("heading", { name: "What are we working on?" });
  const tallerTop = (await heading.boundingBox())?.y;
  await page.setViewportSize({ width: 1_000, height: 520 });
  const shorterTop = (await heading.boundingBox())?.y;

  expect(tallerTop).toBeDefined();
  expect(shorterTop).toBeDefined();
  expect(Math.abs(shorterTop - tallerTop)).toBeLessThanOrEqual(10);

  const surface = page.getByLabel("New task");
  await expect(surface).toHaveJSProperty("scrollTop", 0);
  await page.setViewportSize({ width: 1_000, height: 180 });
  const constrainedGeometry = await surface.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
  }));
  expect(constrainedGeometry.scrollWidth).toBe(constrainedGeometry.clientWidth);
  expect(constrainedGeometry.scrollHeight).toBeGreaterThan(constrainedGeometry.clientHeight);
  await surface.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  expect(await surface.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
});

test("creates a New Task, sends once, streams Chat, tools, and Agent title", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:basic");

  const chat = page.getByLabel("Task chat");
  await expect(page).toHaveURL(/\/task\/task_/);
  await expect(chat.locator("p.chat-user").filter({ hasText: "smoke:basic" })).toHaveText("smoke:basic");
  await expect(chat.getByText("Smoke answer", { exact: true })).toBeVisible();
  await expect(chat.locator(".task-header-title > strong")).toHaveText("Smoke task");
  await page.getByRole("button", { name: "Thought, read file" }).click();
  await expect(page.getByText("Read README.md", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveText("");
});

test("keeps a live permission visible while later ACP updates arrive and resolves it", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:permission");

  const permission = page.getByLabel("Permission request");
  await expect(permission).toBeVisible();
  await expect(page.getByLabel("Task chat").getByText("Permission is still pending", { exact: true })).toBeVisible();
  await expect(permission).toBeVisible();
  await permission.getByRole("button", { name: "Allow once" }).click();

  await expect(permission).toBeHidden();
  await expect(page.getByLabel("Task chat").locator(".chat-agent").last()).toContainText("Permission result: allow-once");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
});

test("redelivers a pending permission after a page reload", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:permission reload");
  await expect(page.getByLabel("Permission request")).toBeVisible();
  await page.reload();

  const permission = page.getByLabel("Permission request");
  await expect(permission).toBeVisible();
  await expect(page.getByLabel("Task chat").locator("p.chat-user")).toHaveCount(1);
  await permission.getByRole("button", { name: "Reject" }).click();
  await expect(permission).toBeHidden();
  await expect(page.getByLabel("Task chat").locator(".chat-agent").last()).toContainText("Permission result: reject-once");
});

test("accepts a steering message while working and lets the user stop the primary prompt", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:hold");
  await expect(page.getByLabel("Task chat").getByText("Waiting for steering", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Task status: Running")).toBeVisible();

  await send(page, "follow up");
  await expect(page.getByLabel("Task chat").locator("p.chat-user").filter({ hasText: "follow up" })).toHaveText("follow up");
  await expect(page.getByLabel("Task chat").getByText("Steering received: follow up", { exact: true })).toBeVisible();
  await page.getByLabel("Stop task").click();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
});

test("retains an unsent prepared New Task across ordinary navigation", async ({ page }) => {
  await openPreparedNewTask(page);
  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill("keep this draft");

  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("region", { name: "Settings" })).toBeVisible();
  await page.getByRole("button", { name: "New task", exact: true }).click();

  await expect(page.getByLabel("New task")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveText("keep this draft");
  await expect(page.getByRole("list", { name: "Tasks" }).getByText("keep this draft", { exact: true })).toHaveCount(0);
});

test("applies Agent options and inserts prepared slash commands", async ({ page }) => {
  await openPreparedNewTask(page);
  await page.getByRole("button", { name: "Test: Balanced", exact: true }).click();
  await page.getByRole("menu", { name: "Test mode" })
    .getByRole("menuitemradio", { name: "Verbose" })
    .click();
  await expect(page.getByRole("button", { name: "Test: Verbose", exact: true })).toBeVisible();

  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill("/");
  const commands = page.getByRole("listbox", { name: "Slash commands" });
  await expect(commands.getByRole("option", { name: /permission/ })).toBeVisible();
  await commands.getByRole("option", { name: /permission/ }).click();
  await expect(editor).toHaveText("/permission ");
});

test("sends an attachment-only first message through the real resolver boundary", async ({ page }) => {
  await openPreparedNewTask(page);
  await page.getByLabel("Add context").click();
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("menu", { name: "Add context" })
    .getByRole("menuitem", { name: /Upload or photo/ })
    .click();
  await (await chooser).setFiles({
    name: "pixel.png",
    mimeType: "image/png",
    buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl2nXcAAAAASUVORK5CYII=", "base64"),
  });

  await expect(page.getByLabel("Attached context").getByLabel("Open pixel.png")).toBeVisible();
  await expect(page.getByLabel("Send message")).toBeEnabled();
  await page.getByLabel("Send message").click();
  await expect(page).toHaveURL(/\/task\/task_/);
  await expect(page.getByLabel("Task chat").getByLabel("Open pixel.png")).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
});

test("closes one permission for every client when either client answers", async ({ page, context }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:permission multi-client");
  await expect(page.getByLabel("Permission request")).toBeVisible();

  const secondPage = await context.newPage();
  await secondPage.goto(page.url());
  await expect(secondPage.getByLabel("Permission request")).toBeVisible();
  await secondPage.getByLabel("Permission request").getByRole("button", { name: "Allow once" }).click();

  await expect(secondPage.getByLabel("Permission request")).toBeHidden();
  await expect(page.getByLabel("Permission request")).toBeHidden();
  await expect(page.getByLabel("Task chat").locator(".chat-agent").last()).toContainText("Permission result: allow-once");
});

test("renders, validates, submits, and persists an Agent question", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:question");

  const question = page.getByRole("form", { name: "Question" });
  await expect(question).toBeVisible();
  await question.getByLabel("Project name").fill("Alpha");
  await question.getByRole("button", { name: "Submit" }).click();

  await expect(question).toBeHidden();
  const answered = page.getByLabel("Question answered");
  await expect(answered).toBeVisible();
  await expect(answered).toContainText("Alpha");
  await expect(page.getByLabel("Task chat").locator(".chat-agent").last()).toContainText("Question result: Alpha");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
});

async function openPreparedNewTask(page) {
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
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveAttribute("contenteditable", "true");
  await expect(page.getByLabel("Send message")).toBeDisabled();
}

async function send(page, text) {
  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill(text);
  await page.getByLabel("Send message").click();
}
