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

test("keeps shared typography when an App Shell supplies body defaults", async ({ page }) => {
  await openPreparedNewTask(page);

  const body = page.locator("body");
  await expect(body).toHaveCSS("font-family", /Inter Variable/);
  await expect(body).toHaveCSS("font-size", "14px");

  // VS Code supplies lower-priority body typography; OpenAIDE must own the final App Shell result.
  await page.addStyleTag({
    content: ':where(body) { font-family: "Segoe UI", sans-serif; font-size: 13px; }',
  });

  await expect(body).toHaveCSS("font-family", /Inter Variable/);
  await expect(body).toHaveCSS("font-size", "14px");
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
  const readStep = chat.locator(".activity-step").filter({
    has: page.locator(".activity-step-semantic-action", { hasText: /^Read$/ }),
  }).filter({
    has: page.locator(".activity-step-semantic-subject", { hasText: /^README\.md$/ }),
  });
  await expect(readStep).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveText("");
});

test("keeps a Task actions popup interactive after the pointer leaves its row", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:basic");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();

  const row = page.getByRole("listitem").filter({ hasText: "Smoke task" }).first();
  await page.evaluate(() => {
    window.__taskPreviewInsertions = 0;
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (node instanceof Element
            && (node.matches(".task-preview-popover") || node.querySelector(".task-preview-popover"))) {
            window.__taskPreviewInsertions += 1;
          }
        }
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
  await row.hover();
  await page.waitForTimeout(250);
  await row.getByRole("button", { name: "Task actions for Smoke task" }).click();
  await page.waitForTimeout(1_100);

  const menu = page.getByRole("menu", { name: "Task actions for Smoke task" });
  await expect(menu).toBeVisible();
  expect(await page.evaluate(() => window.__taskPreviewInsertions)).toBe(0);
  await expect(menu).toHaveCSS("transition-duration", "0.045s");
  await expect(page.locator(".task-preview-popover")).toHaveCount(0);
  await expect(page.locator("#openaide-popup-layer").getByRole("menu")).toHaveCount(1);
  const bounds = await menu.boundingBox();
  expect(bounds).not.toBeNull();

  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height + 8);
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height - 2);
  const hitRole = await page.evaluate(({ x, y }) =>
    document.elementFromPoint(x, y)?.closest("[role]")?.getAttribute("role"), {
    x: bounds.x + bounds.width / 2,
    y: bounds.y + bounds.height - 2,
  });
  expect(hitRole).toBe("menu");

  const taskUrl = page.url();
  await page.mouse.click(bounds.x + 2, bounds.y + 2);
  await expect(page).toHaveURL(taskUrl);
  await expect(menu).toBeVisible();
});

test("shows a complete long Task title in a compact hover preview", async ({ page }) => {
  const title = "A deliberately long task title segment that remains readable in the compact hover preview. ".repeat(12).trim();
  await openPreparedNewTask(page);
  await send(page, "smoke:long-title");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await page.setViewportSize({ width: 1_662, height: 215 });

  const row = page.getByRole("listitem").filter({ hasText: title }).first();
  await row.hover();
  const preview = page.locator(".task-preview-popover");
  await expect(preview).toBeVisible();
  await expect(preview.locator("header strong")).toHaveText(title);

  const geometry = await preview.evaluate((element) => {
    const titleElement = element.querySelector("header strong");
    const bounds = element.getBoundingClientRect();
    return {
      bottom: bounds.bottom,
      height: bounds.height,
      titleClientHeight: titleElement.clientHeight,
      titleClientWidth: titleElement.clientWidth,
      titleScrollTop: titleElement.scrollTop,
      titleScrollHeight: titleElement.scrollHeight,
      titleScrollWidth: titleElement.scrollWidth,
      width: bounds.width,
    };
  });
  expect(geometry.width).toBeLessThanOrEqual(380);
  expect(geometry.bottom).toBeLessThanOrEqual(207);
  expect(geometry.titleScrollWidth).toBeLessThanOrEqual(geometry.titleClientWidth);
  expect(geometry.titleClientHeight).toBeLessThanOrEqual(110);
  expect(geometry.titleScrollHeight).toBeGreaterThan(geometry.titleClientHeight);
  const titleWrap = preview.locator(".task-preview-title-wrap");
  await expect(titleWrap).toHaveAttribute("data-more-below", "true");
  expect(await titleWrap.evaluate((element) => getComputedStyle(element, "::after").opacity)).toBe("1");
  await preview.locator(".task-preview-title").evaluate((element) => {
    element.scrollTop = element.scrollHeight;
    element.dispatchEvent(new Event("scroll"));
  });
  expect(await preview.locator(".task-preview-title").evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  await expect(titleWrap).toHaveAttribute("data-more-below", "false");
  await expect.poll(() => titleWrap.evaluate((element) => getComputedStyle(element, "::after").opacity)).toBe("0");
  await expect(preview.getByText("Project", { exact: true })).toBeVisible();
  await expect(preview.getByText("Location", { exact: true })).toBeVisible();
});

test("recovers an open Task composer once after client liveness expires", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:basic");

  const editor = page.getByRole("textbox", { name: "Message" });
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await editor.fill("draft survives recovery");
  await startComposerPlaceholderTrace(page);
  const stopExpiryFault = await reportClientLivenessExpiredOnNextHeartbeat(page);
  try {
    const transitions = await waitForComposerPlaceholderRecovery(page);
    expect(transitions).toEqual([
      "Send follow-up",
      "Reconnecting. Draft is saved here.",
      "Send follow-up",
    ]);
    await expect(editor).toHaveText("draft survives recovery");
  } finally {
    await stopExpiryFault();
  }
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

test("settles the task when an accepted steering message ends", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:hold");
  await expect(page.getByLabel("Task chat").getByText("Waiting for steering", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Task status: Running")).toBeVisible();

  await send(page, "follow up");
  await expect(page.getByLabel("Task chat").locator("p.chat-user").filter({ hasText: "follow up" })).toHaveText("follow up");
  await expect(page.getByLabel("Task chat").getByText("Steering received: follow up", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await expect(page.getByLabel("Stop task")).toBeHidden();
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
  await page.getByRole("button", { name: "Balanced", exact: true }).click();
  await page.getByRole("menu", { name: "Test mode" })
    .getByRole("menuitemradio", { name: "Verbose" })
    .click();
  await expect(page.getByRole("button", { name: "Verbose", exact: true })).toBeVisible();

  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill("/");
  const commands = page.getByRole("listbox", { name: "Slash commands" });
  await expect(commands.getByRole("option", { name: /permission/ })).toBeVisible();
  await commands.getByRole("option", { name: /permission/ }).click();
  await expect(editor).toHaveText("/permission ");
});

test("sends an attachment-only first message through the real resolver boundary", async ({ page }) => {
  await openPreparedNewTask(page);
  await page.getByRole("button", { name: "Add context" }).click();
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("menu", { name: "Add context" })
    .getByRole("menuitem", { name: /Attach images/ })
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
  await expect(page.getByLabel("Task chat").getByLabel("Open Image")).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
});

test("uploads a 2 MiB file and sends it with the first New Task message", async ({ page }) => {
  await openPreparedNewTask(page);
  await page.getByRole("button", { name: "Add context" }).click();
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("menu", { name: "Add context" })
    .getByRole("menuitem", { name: /Attach files/ })
    .click();
  await (await chooser).setFiles({
    name: "two-megabytes.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.alloc(2 * 1024 * 1024, 7),
  });

  const attached = page.getByLabel("Attached context");
  await expect(attached.getByRole("button", { name: "Remove two-megabytes.bin" })).toBeVisible();
  await expect(attached.getByLabel("Uploading two-megabytes.bin")).toHaveCount(0);
  await expect(attached.getByText("two-megabytes.bin", { exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "Message" }).fill("smoke:file attachment");
  await page.getByLabel("Send message").click();

  await expect(page).toHaveURL(/\/task\/task_/);
  await expect(page.getByLabel("Task chat").locator("p.chat-user")).toHaveText("smoke:file attachment");
  await expect(page.getByText("Reselect attachments from the file browser before sending.")).toHaveCount(0);
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();

  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download two-megabytes.bin" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("two-megabytes.bin");
  const stream = await download.createReadStream();
  let downloadedBytes = 0;
  for await (const chunk of stream) downloadedBytes += chunk.length;
  expect(downloadedBytes).toBe(2 * 1024 * 1024);
});

test("keeps Images and files in one composer attachment list", async ({ page }) => {
  await openPreparedNewTask(page);
  await page.getByRole("button", { name: "Add context" }).click();
  let chooser = page.waitForEvent("filechooser");
  await page.getByRole("menuitem", { name: /Attach images/ }).click();
  await (await chooser).setFiles({
    name: "pixel.png",
    mimeType: "image/png",
    buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64"),
  });
  await expect(page.getByRole("menu", { name: "Add context" })).toHaveCount(0);
  await page.getByRole("button", { name: "Add context" }).click();
  chooser = page.waitForEvent("filechooser");
  await page.getByRole("menuitem", { name: /Attach files/ }).click();
  await (await chooser).setFiles({
    name: "notes.md",
    mimeType: "text/markdown",
    buffer: Buffer.from("attachment list"),
  });

  const list = page.getByLabel("Attached context").locator(".composer-attachment-list");
  await expect(list).toHaveCount(1);
  await expect(list.locator(".composer-attachment-tile")).toHaveCount(2);
  const tops = await list.locator(".composer-attachment-tile").evaluateAll((tiles) =>
    tiles.map((tile) => Math.round(tile.getBoundingClientRect().top)));
  expect(new Set(tops).size).toBe(1);
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

async function startComposerPlaceholderTrace(page) {
  await page.evaluate(() => {
    const expected = [
      "Send follow-up",
      "Reconnecting. Draft is saved here.",
      "Send follow-up",
    ];
    const transitions = [];
    const sample = () => {
      const editor = document.querySelector('[role="textbox"][aria-label="Message"]');
      const value = editor instanceof HTMLElement
        ? editor.getAttribute("data-placeholder") ?? "missing"
        : "missing";
      if (transitions.at(-1) !== value) transitions.push(value);
      const completed = expected.every((item, index) => transitions[index] === item);
      if (completed) {
        observer.disconnect();
        window.__openaideComposerPlaceholderTrace.completed = true;
      }
    };
    const observer = new MutationObserver(sample);
    window.__openaideComposerPlaceholderTrace = { completed: false, transitions };
    sample();
    if (!window.__openaideComposerPlaceholderTrace.completed) {
      observer.observe(document.body, {
        attributes: true,
        attributeFilter: ["data-placeholder"],
        childList: true,
        subtree: true,
      });
    }
  });
}

async function waitForComposerPlaceholderRecovery(page) {
  await page.waitForFunction(() => window.__openaideComposerPlaceholderTrace?.completed === true);
  return page.evaluate(() => window.__openaideComposerPlaceholderTrace?.transitions ?? []);
}

async function reportClientLivenessExpiredOnNextHeartbeat(page) {
  const probePattern = "**/__openaide-app-server/probe";
  let pendingError;
  let expiredSessionId;
  let resolveHeartbeat;
  let resolveInjected;
  const observed = [];
  const heartbeat = new Promise((resolve) => { resolveHeartbeat = resolve; });
  const injected = new Promise((resolve) => { resolveInjected = resolve; });
  const injectExpiry = async (route) => {
    const request = route.request();
    if (request.method() === "POST") {
      const body = request.postDataJSON();
      observed.push(`POST:${body?.transport ?? "unknown"}:${body?.message?.method ?? "no-method"}`);
      if (
        !expiredSessionId
        && body?.transport === "send"
        && body.message?.method === "client/heartbeat"
      ) {
        // Reproduce the App Server's real liveness-expiry response at the web proxy boundary.
        expiredSessionId = body.sessionId;
        pendingError = {
          jsonrpc: "2.0",
          id: body.message.id,
          error: {
            error: {
              code: "notInitialized",
              message: "client/initialize must succeed before product requests",
            },
          },
        };
        await route.fulfill({ status: 204, body: "" });
        resolveHeartbeat();
        return;
      }
      if (body?.sessionId === expiredSessionId) {
        // The synthetic response did not reach the real server, so quarantine this obsolete session.
        await route.fulfill({ status: 204, body: "" });
        return;
      }
    }
    if (request.method() === "GET" && pendingError) {
      observed.push("GET:inject");
      const after = Number(request.headers()["x-openaide-after"] ?? "0");
      const message = pendingError;
      pendingError = undefined;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ frames: [{ sequence: after + 1, message }] }),
      });
      resolveInjected();
      return;
    }
    if (
      request.method() === "GET"
      && request.headers()["x-openaide-session-id"] === expiredSessionId
    ) {
      await route.fulfill({ status: 204, body: "" });
      return;
    }
    await route.continue();
  };
  await page.route(probePattern, injectExpiry);
  try {
    await Promise.race([
      heartbeat,
      new Promise((_, reject) => setTimeout(() => reject(new Error("Heartbeat was not observed")), 10_000)),
    ]);
    await page.evaluate(() => window.dispatchEvent(new Event("online")));
    await Promise.race([
      injected,
      new Promise((_, reject) => setTimeout(() => reject(new Error(
        `Heartbeat expiry was not injected: ${observed.slice(-20).join(", ")}`,
      )), 10_000)),
    ]);
    return () => page.unroute(probePattern, injectExpiry);
  } catch (error) {
    await page.unroute(probePattern, injectExpiry);
    throw error;
  }
}
