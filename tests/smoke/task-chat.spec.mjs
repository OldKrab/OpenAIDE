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

test("pastes Windows multiline text without blank lines and undoes it as one edit", async ({ context, page }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: harness.baseUrl });
  await openPreparedNewTask(page);

  const composer = page.getByRole("textbox", { name: "Message" });
  await page.evaluate(async () => navigator.clipboard.writeText("alpha\r\nbeta\r\ngamma"));
  await composer.focus();
  await page.keyboard.press("Control+V");

  await expect.poll(() => composer.evaluate((element) => element.innerText))
    .toBe("alpha\nbeta\ngamma");

  await page.keyboard.press("Control+Z");

  await expect.poll(() => composer.evaluate((element) => element.innerText)).toBe("");
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

test("keeps wrapped Plan entries readable at desktop and narrow widths", async ({ page }) => {
  await page.setViewportSize({ width: 1_180, height: 700 });
  await openPreparedNewTask(page);
  await send(page, "smoke:long-plan-layout");
  await expect(page.getByText("Long Plan rendered", { exact: true })).toBeVisible();

  const planTrigger = page.locator(".task-plan-drawer-trigger");
  await expect(planTrigger).toBeVisible();
  if (await planTrigger.getAttribute("aria-expanded") !== "true") await planTrigger.click();
  const desktopPlan = page.locator(".task-plan-drawer[data-open='true'] .agent-plan");
  await expect(desktopPlan).toBeVisible();
  expectPlanEntriesReadable(await planEntryLayout(desktopPlan));

  await page.setViewportSize({ width: 1_800, height: 900 });
  const widePlan = page.locator(".task-plan-column .agent-plan");
  await expect(widePlan).toBeVisible();
  expectPlanEntriesReadable(await planEntryLayout(widePlan));

  await page.setViewportSize({ width: 390, height: 844 });
  const narrowPlan = page.locator(".task-plan-drawer[data-open='true'] .agent-plan");
  await expect(narrowPlan).toBeVisible();
  expectPlanEntriesReadable(await planEntryLayout(narrowPlan));
});

test("creates a New Task, sends once, streams Chat, tools, and Agent title", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:basic");

  const chat = page.getByLabel("Task chat");
  await expect(page).toHaveURL(/\/task\/task_/);
  const userMessage = chat.locator("p.chat-user").filter({ hasText: "smoke:basic" });
  await expect(userMessage).toHaveText("smoke:basic");
  const userMessageAlignment = await userMessage.evaluate((element) => {
    const block = element.closest(".chat-user-block");
    const virtualRow = element.closest(".message-list-virtual-row");
    if (!block || !virtualRow) throw new Error("Virtualized User message structure is incomplete.");
    return virtualRow.getBoundingClientRect().right - block.getBoundingClientRect().right;
  });
  expect(userMessageAlignment).toBeCloseTo(0, 0);
  await expect(chat.getByText("Smoke answer", { exact: true })).toBeVisible();
  await expect(chat.locator(".task-header-title > strong")).toHaveText("Smoke task");
  await page.getByRole("button", { name: "Read file, thought" }).click();
  const readStep = chat.locator(".activity-step").filter({
    has: page.locator(".activity-step-semantic-action", { hasText: /^Read$/ }),
  }).filter({
    has: page.locator(".activity-step-semantic-subject", { hasText: /^README\.md$/ }),
  });
  await expect(readStep).toBeVisible();
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveText("");
});

test("keeps an Agent link clickable while its message is streaming", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:streaming-link-click");

  const link = page.getByRole("link", { name: "Streaming link" });
  await expect(link).toBeVisible();
  await page.evaluate(() => {
    window.__openaideStreamingLinkClicks = 0;
    document.addEventListener("click", (event) => {
      if (event.target instanceof Element && event.target.closest("a")?.textContent === "Streaming link") {
        event.preventDefault();
        window.__openaideStreamingLinkClicks += 1;
      }
    }, { once: true });
  });

  const bounds = await link.boundingBox();
  expect(bounds).not.toBeNull();
  const point = { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
  await page.mouse.move(point.x, point.y);
  await page.mouse.down();
  await expect(page.getByText("Second paragraph arrives while the link is pressed.", { exact: true })).toBeVisible();
  await page.mouse.up();

  await expect.poll(() => page.evaluate(() => window.__openaideStreamingLinkClicks)).toBe(1);
});

test("keeps the context meter on the composer's rounded edge as a draft grows", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 640 });
  await openPreparedNewTask(page);
  await send(page, "smoke:context-usage-curve");
  await expect(page.getByText("Context usage rendered", { exact: true })).toBeVisible();

  const editor = page.getByRole("textbox", { name: "Message" });
  const composer = page.locator(".composer");
  const initialHeight = await composer.evaluate((element) => element.getBoundingClientRect().height);
  await editor.fill(Array.from({ length: 80 }, (_, index) => `line ${index + 1}`).join("\n"));

  const meterEdge = page.locator(".context-usage-edge");
  const geometry = await composer.evaluate((element) => ({
    height: element.getBoundingClientRect().height,
    radius: getComputedStyle(element).borderTopRightRadius,
  }));
  const edgeGeometry = await measureContextEdge(meterEdge, Number.parseFloat(geometry.radius));

  expect(geometry.height).toBeGreaterThan(initialHeight);
  expect(edgeGeometry.height).toBe(geometry.height);
  expect(edgeGeometry.viewBoxHeight).toBe(geometry.height);
  expect(edgeGeometry.width).toBe(20);
  expect(edgeGeometry.straightThickness).toBeCloseTo(1.5, 1);
  expect(edgeGeometry.cornerEndThickness).toBeGreaterThan(0.1);
  expect(edgeGeometry.cornerEndThickness).toBeLessThan(0.75);
  expect(edgeGeometry.cornerThicknesses).toEqual(
    edgeGeometry.cornerThicknesses.toSorted((a, b) => b - a),
  );
  expect(edgeGeometry.cornerThicknesses).toEqual([
    expect.closeTo(1.3, 1),
    expect.closeTo(0.88, 1),
    expect.closeTo(0.45, 1),
    expect.closeTo(0.26, 1),
  ]);

  const meter = page.locator(".context-usage-meter");
  await meter.hover();
  const tooltip = page.getByRole("tooltip", { name: "Context used: 12%" });
  await expect(tooltip).toBeVisible();
  const tooltipGeometry = await tooltip.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return { centerY: bounds.y + bounds.height / 2, right: bounds.right };
  });
  const composerBounds = await composer.boundingBox();
  expect(composerBounds).not.toBeNull();
  expect(tooltipGeometry.centerY).toBeCloseTo(
    composerBounds.y + composerBounds.height * 0.88,
    0,
  );
  const edgeBounds = await meterEdge.boundingBox();
  expect(edgeBounds).not.toBeNull();
  // Visibility can become observable while the 100 ms hover transform is still settling.
  await expect.poll(async () => {
    const tooltipBounds = await tooltip.boundingBox();
    const currentEdgeBounds = await meterEdge.boundingBox();
    if (!tooltipBounds || !currentEdgeBounds) return 0;
    return currentEdgeBounds.x + currentEdgeBounds.width - tooltipBounds.x - tooltipBounds.width;
  }).toBeGreaterThan(4);
  const settledTooltipBounds = await tooltip.boundingBox();
  const settledEdgeBounds = await meterEdge.boundingBox();
  expect(settledTooltipBounds).not.toBeNull();
  expect(settledEdgeBounds).not.toBeNull();
  const tooltipGap = settledEdgeBounds.x + settledEdgeBounds.width
    - settledTooltipBounds.x - settledTooltipBounds.width;
  expect(tooltipGap).toBeLessThan(7);

  await page.setViewportSize({ width: 1_200, height: 800 });
  const desktopGeometry = await composer.evaluate((element) => ({
    height: element.getBoundingClientRect().height,
    radius: getComputedStyle(element).borderTopRightRadius,
  }));
  const desktopEdgeGeometry = await measureContextEdge(
    meterEdge,
    Number.parseFloat(desktopGeometry.radius),
  );
  expect(desktopEdgeGeometry.height).toBe(desktopGeometry.height);
  expect(desktopEdgeGeometry.viewBoxHeight).toBe(desktopGeometry.height);
  expect(desktopEdgeGeometry.width).toBe(20);
  expect(desktopEdgeGeometry.straightThickness).toBeCloseTo(1.5, 1);
  expect(desktopEdgeGeometry.cornerEndThickness).toBeGreaterThan(0.1);
  expect(desktopEdgeGeometry.cornerEndThickness).toBeLessThan(0.75);
  expect(desktopEdgeGeometry.cornerThicknesses).toEqual(
    desktopEdgeGeometry.cornerThicknesses.toSorted((a, b) => b - a),
  );
});

test("keeps collapsed tool rows equally spaced with and without details", async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 320 });
  await openPreparedNewTask(page);
  await send(page, "smoke:activity-row-spacing");
  const chat = page.getByLabel("Task chat");
  await expect(chat.getByText("Activity rows rendered", { exact: true })).toBeVisible();

  const activity = chat.locator(".activity-group").filter({
    hasText: "Wait for subagents",
  });
  const activityTrigger = activity.locator(":scope > .activity-disclosure-trigger");
  const openingOverlap = await maximumVirtualRowOverlapDuring(page, async () => {
    await activityTrigger.click();
  });
  expect(openingOverlap).toBeLessThanOrEqual(0.5);
  const rows = activity.locator(".activity-step");
  await expect(rows).toHaveCount(2);

  const heights = await rows.evaluateAll((elements) =>
    elements.map((element) => element.getBoundingClientRect().height));
  expect(heights).toEqual([26, 26]);

  const constrainedGeometry = await activity.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
  }));
  expect(constrainedGeometry.scrollWidth).toBe(constrainedGeometry.clientWidth);

  await page.setViewportSize({ width: 1_200, height: 800 });
  const desktopHeights = await rows.evaluateAll((elements) =>
    elements.map((element) => element.getBoundingClientRect().height));
  expect(desktopHeights).toEqual([26, 26]);

  await rows.first().getByRole("button").click();
  await expect(rows.first().getByText("fixture output", { exact: true })).toBeVisible();

  const closingOverlap = await maximumVirtualRowOverlapDuring(page, async () => {
    await activityTrigger.click();
  });
  expect(closingOverlap).toBeLessThanOrEqual(0.5);
});

test("copies fenced Markdown code independently at desktop and constrained widths", async ({ context, page }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: harness.baseUrl });
  await openPreparedNewTask(page);
  await send(page, "smoke:code-block-copy");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();

  const codeBlocks = page.locator(".agent-markdown-code-block");
  await expect(codeBlocks).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Copy code" })).toHaveCount(2);
  await codeBlocks.nth(1).getByRole("button", { name: "Copy code" }).click();
  await expect(codeBlocks.nth(1).getByRole("button", { name: "Code copied" })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("def second():\n    return 2");
  await expect(codeBlocks.nth(0).getByRole("button", { name: "Copy code" })).toBeVisible();

  await page.setViewportSize({ width: 360, height: 640 });
  const geometry = await codeBlocks.nth(1).evaluate((element) => {
    const pre = element.querySelector("pre");
    const button = element.querySelector("button");
    const code = element.querySelector("code");
    if (!pre || !button || !code) throw new Error("Code block structure is incomplete.");
    const preBounds = pre.getBoundingClientRect();
    const buttonBounds = button.getBoundingClientRect();
    const codeBounds = code.getBoundingClientRect();
    return {
      blockClientWidth: element.clientWidth,
      blockScrollWidth: element.scrollWidth,
      buttonDoesNotOverlapCode:
        buttonBounds.right <= codeBounds.left
        || buttonBounds.left >= codeBounds.right
        || buttonBounds.bottom <= codeBounds.top
        || buttonBounds.top >= codeBounds.bottom,
      buttonInsidePre: buttonBounds.right <= preBounds.right && buttonBounds.left >= preBounds.left,
      firstLineInset: codeBounds.top - preBounds.top,
    };
  });
  expect(geometry.blockScrollWidth).toBe(geometry.blockClientWidth);
  expect(geometry.firstLineInset).toBeLessThanOrEqual(12);
  expect(geometry.buttonDoesNotOverlapCode).toBe(true);
  expect(geometry.buttonInsidePre).toBe(true);
});

test("fits and inspects a Mermaid diagram inline and expanded without source dead space", async ({ page }) => {
  await page.setViewportSize({ width: 1_200, height: 800 });
  await openPreparedNewTask(page);
  await send(page, "smoke:mermaid-preview");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();

  const diagram = page.locator(".agent-mermaid");
  await expect(diagram.locator(".agent-mermaid-image")).toBeVisible({ timeout: 30_000 });
  const copySource = diagram.getByRole("button", { name: "Copy source" });
  await expect(copySource).toBeVisible();
  expect((await copySource.boundingBox())?.width).toBeLessThanOrEqual(30);
  await expectPreviewToFit(page, diagram.locator(".attachment-preview-stage"));
  const inlineImage = diagram.locator(".agent-mermaid-image");
  const inlineBounds = await inlineImage.boundingBox();
  expect(inlineBounds).not.toBeNull();
  await page.mouse.move(
    inlineBounds.x + inlineBounds.width / 2,
    inlineBounds.y + inlineBounds.height / 2,
  );
  await page.mouse.wheel(0, -100);
  await expect(diagram.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("125%");
  await expect.poll(
    async () => (await inlineImage.boundingBox())?.width,
  ).toBeGreaterThan(inlineBounds.width);
  const inlineAfter = await inlineImage.evaluate((element) => getComputedStyle(element).transform);
  await page.mouse.down();
  await page.mouse.move(
    inlineBounds.x + inlineBounds.width / 2 + 40,
    inlineBounds.y + inlineBounds.height / 2 + 25,
  );
  await page.mouse.up();
  await expect.poll(
    () => inlineImage.evaluate((element) => getComputedStyle(element).transform),
  ).not.toBe(inlineAfter);
  await diagram.getByRole("button", { name: "Reset diagram zoom" }).click();
  const zoomIn = diagram.getByRole("button", { name: "Zoom diagram in" });
  for (let step = 0; step < 16; step += 1) await zoomIn.click();
  await expect(diagram.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("500%");
  await expect(zoomIn).toBeEnabled();
  await zoomIn.click();
  await expect(diagram.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("750%");
  await diagram.getByRole("button", { name: "Reset diagram zoom" }).click();

  await diagram.getByRole("button", { name: "View diagram source" }).click();
  const sourceGeometry = await diagram.evaluate((element) => {
    const code = element.querySelector("code");
    const source = element.querySelector(".agent-mermaid-source");
    const actions = element.querySelector(".agent-mermaid-source-actions");
    if (!code || !source || !actions) throw new Error("Mermaid source structure is incomplete.");
    const firstLine = document.createRange();
    const firstLineLength = code.textContent?.indexOf("\n") ?? -1;
    firstLine.setStart(code.firstChild, 0);
    firstLine.setEnd(code.firstChild, firstLineLength < 0 ? code.textContent.length : firstLineLength);
    const codeBounds = firstLine.getBoundingClientRect();
    const sourceBounds = source.getBoundingClientRect();
    const actionsBounds = actions.getBoundingClientRect();
    return {
      firstLineInset: codeBounds.top - sourceBounds.top,
      firstLineDoesNotOverlapActions:
        codeBounds.right <= actionsBounds.left
        || codeBounds.left >= actionsBounds.right
        || codeBounds.bottom <= actionsBounds.top
        || codeBounds.top >= actionsBounds.bottom,
    };
  });
  expect(sourceGeometry.firstLineInset).toBeLessThanOrEqual(20);
  expect(sourceGeometry.firstLineDoesNotOverlapActions).toBe(true);

  await diagram.getByRole("button", { name: "Show diagram" }).click();
  await diagram.getByRole("button", { name: "Expand diagram" }).click();
  const preview = page.getByRole("dialog", { name: "Mermaid diagram preview" });
  await expect(preview.locator(".attachment-preview-image")).toBeVisible();
  await expectPreviewToFit(page, preview.locator(".attachment-preview-stage"));

  const image = preview.locator(".attachment-preview-image");
  const before = await image.boundingBox();
  expect(before).not.toBeNull();
  const focus = { x: before.x + before.width / 2, y: before.y + before.height / 2 };
  await page.mouse.move(focus.x, focus.y);
  await page.mouse.wheel(0, -100);
  await expect(page.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("125%");
  const after = await image.boundingBox();
  expect(after).not.toBeNull();
  expect(after.x + after.width / 2).toBeCloseTo(focus.x, 0);
  expect(after.y + after.height / 2).toBeCloseTo(focus.y, 0);

  await preview.getByRole("button", { name: "Reset diagram zoom" }).click();
  const previewZoomIn = preview.getByRole("button", { name: "Zoom diagram in" });
  for (let step = 0; step < 16; step += 1) await previewZoomIn.click();
  await expect(preview.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("500%");
  await expect(previewZoomIn).toBeEnabled();
  await previewZoomIn.click();
  await expect(preview.getByRole("button", { name: "Reset diagram zoom" })).toHaveText("750%");

  await page.getByRole("button", { name: "Close diagram preview" }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await diagram.getByRole("button", { name: "Expand diagram" }).click();
  const narrowPreview = page.getByRole("dialog", { name: "Mermaid diagram preview" });
  await expect(narrowPreview.locator(".attachment-preview-image")).toBeVisible();
  await expectPreviewToFit(page, narrowPreview.locator(".attachment-preview-stage"));
  const dialogGeometry = await narrowPreview.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
  }));
  expect(dialogGeometry.scrollWidth).toBe(dialogGeometry.clientWidth);
});

test("waits for the Agent message to complete before rendering Mermaid", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:mermaid-preview");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  const chat = page.getByLabel("Task chat");
  await expect(chat.locator(".agent-mermaid")).toHaveCount(1, { timeout: 30_000 });

  await send(page, "smoke:mermaid-streaming");

  await expect(chat.locator("code.language-mermaid")).toBeVisible();
  await expect(chat.locator(".agent-mermaid")).toHaveCount(1);
  await expect(page.getByLabel("Task status: Ready")).toBeVisible({ timeout: 10_000 });
  await expect(chat.locator(".agent-mermaid")).toHaveCount(2, { timeout: 30_000 });
});

async function expectPreviewToFit(page, stage) {
  await expect.poll(async () => stage.evaluate((stageElement) => {
    const image = stageElement.querySelector("img");
    if (!image) return false;
    const stageBounds = stageElement.getBoundingClientRect();
    const imageBounds = image.getBoundingClientRect();
    return imageBounds.left >= stageBounds.left
      && imageBounds.right <= stageBounds.right
      && imageBounds.top >= stageBounds.top
      && imageBounds.bottom <= stageBounds.bottom;
  })).toBe(true);
}

test("quotes selected rendered Chat text into the Composer at desktop and narrow widths", async ({ page }) => {
  await page.setViewportSize({ width: 1_200, height: 800 });
  await openPreparedNewTask(page);
  await send(page, "smoke:quote-selection");
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();

  const chat = page.getByLabel("Task chat");
  const agent = chat.locator('[data-quote-source="agent"]').filter({ hasText: "linked text" });
  const linkSelectionEnd = await selectRenderedText(agent.getByRole("link", { name: "linked text" }));

  const quote = page.getByRole("button", { name: "Quote selected text" });
  await expect(quote).toBeVisible();
  const quoteAtSelectionEnd = await quote.boundingBox();
  expect(quoteAtSelectionEnd).not.toBeNull();
  expect(Math.abs(quoteAtSelectionEnd.x + quoteAtSelectionEnd.width / 2 - linkSelectionEnd.left)).toBeLessThanOrEqual(1);
  expect(Math.abs(quoteAtSelectionEnd.y + quoteAtSelectionEnd.height + 8 - linkSelectionEnd.top)).toBeLessThanOrEqual(1);
  await quote.click();

  const composer = page.getByRole("textbox", { name: "Message" });
  await expect.poll(() => composer.evaluate(composerText)).toBe("> linked text\n");
  await expect(composer).toBeFocused();

  await selectRenderedText(agent.locator("p code").first());
  await expect(quote).toBeVisible();
  await quote.click();
  await expect.poll(() => composer.evaluate(composerText)).toBe("> linked text\n> inline\n");

  await page.setViewportSize({ width: 360, height: 640 });
  await selectRenderedText(agent.locator("pre code").first());
  await expect(quote).toBeVisible();
  const quoteBounds = await quote.boundingBox();
  expect(quoteBounds).not.toBeNull();
  expect(quoteBounds.x).toBeGreaterThanOrEqual(0);
  expect(quoteBounds.x + quoteBounds.width).toBeLessThanOrEqual(360);
  expect(quoteBounds.height).toBeGreaterThanOrEqual(36);
  await quote.click();

  await expect.poll(() => composer.evaluate(composerText)).toBe(
    "> linked text\n> inline\n> const selected = true;\n> const next = 2;\n",
  );
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
  await expect(preview.locator("small").filter({ hasText: /^Project$/ })).toBeVisible();
  await expect(preview.locator("small").filter({ hasText: /^Location$/ })).toBeVisible();
});

test("recovers an open Task composer once after client liveness expires", async ({ page }) => {
  await openPreparedNewTask(page);
  await send(page, "smoke:basic");

  const editor = page.getByRole("textbox", { name: "Message" });
  await expect(page.getByLabel("Task status: Ready")).toBeVisible();
  await editor.fill("draft survives recovery");
  await startComposerConnectionTrace(page);
  const stopExpiryFault = await reportClientLivenessExpiredOnNextHeartbeat(page);
  try {
    const transitions = await waitForComposerConnectionRecovery(page);
    expect(transitions).toEqual([
      "ready",
      "reconnecting",
      "ready",
    ]);
    await expect(editor).toHaveAttribute("data-placeholder", "Send follow-up");
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
  await expect(page.getByRole("main", { name: "Settings" })).toBeVisible();
  await page.getByRole("button", { name: "Close settings" }).click();

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
  const [composerBounds, commandBounds] = await Promise.all([
    page.locator(".composer").boundingBox(),
    commands.boundingBox(),
  ]);
  expect(composerBounds).not.toBeNull();
  expect(commandBounds).not.toBeNull();
  expect(commandBounds.width).toBeLessThanOrEqual(composerBounds.width);
  await commands.getByRole("option", { name: /permission/ }).click();
  await expect(editor).toHaveText("/permission ");
});

test("keeps text typed at a slash-command boundary outside the highlighted token", async ({ page }) => {
  await openPreparedNewTask(page);
  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill("/");
  await page.getByRole("listbox", { name: "Slash commands" })
    .getByRole("option", { name: /permission/ })
    .click();

  const command = editor.locator(".composer-command-token");
  await command.evaluate((token) => {
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(token);
    range.collapse(false);
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
  await page.keyboard.type(" details");

  await expect(command).toHaveText("/permission");
  await expect(editor).toHaveText("/permission details ");
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

async function measureContextEdge(meterEdge, radius) {
  return meterEdge.evaluate((element, curveRadius) => {
    const path = element.querySelector(".context-usage-edge-track");
    if (!(element instanceof SVGSVGElement) || !(path instanceof SVGGeometryElement)) {
      throw new Error("Context edge SVG geometry is missing");
    }

    const containsPoint = (x, y) => {
      const point = element.createSVGPoint();
      point.x = x;
      point.y = y;
      return path.isPointInFill(point);
    };
    const filledRun = (start, end, pointAt) => {
      const step = 0.01;
      let first;
      let last;
      for (let position = start; position <= end; position += step) {
        const [x, y] = pointAt(position);
        if (!containsPoint(x, y)) continue;
        first ??= position;
        last = position;
      }
      return first === undefined ? 0 : last - first + step;
    };

    const bounds = element.getBoundingClientRect();
    const curveStartX = 20 - curveRadius;
    const radialThickness = (progress) => {
      const angle = progress * Math.PI / 2;
      return filledRun(curveRadius - 2, curveRadius, (sampleRadius) => [
        curveStartX + sampleRadius * Math.cos(angle),
        curveRadius - sampleRadius * Math.sin(angle),
      ]);
    };
    return {
      height: bounds.height,
      width: bounds.width,
      viewBoxHeight: element.viewBox.baseVal.height,
      straightThickness: filledRun(0, 20, (x) => [x, bounds.height / 2]),
      cornerEndThickness: filledRun(0, curveRadius, (y) => [curveStartX + 0.05, y]),
      cornerThicknesses: [0.25, 0.5, 0.75, 0.95].map(radialThickness),
    };
  }, radius);
}

/** Samples every animation frame so transient virtual-row collisions cannot self-heal unnoticed. */
async function maximumVirtualRowOverlapDuring(page, action) {
  await page.evaluate(() => {
    const state = { active: true, maximum: 0 };
    window.__openaideVirtualRowOverlap = state;
    const sample = () => {
      const rows = [...document.querySelectorAll(".message-list-virtual-row")]
        .map((element) => ({
          index: Number(element.getAttribute("data-index")),
          bounds: element.getBoundingClientRect(),
        }))
        .sort((left, right) => left.index - right.index);
      for (let index = 0; index < rows.length - 1; index += 1) {
        state.maximum = Math.max(
          state.maximum,
          rows[index].bounds.bottom - rows[index + 1].bounds.top,
        );
      }
      if (state.active) requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });

  await action();
  await page.waitForTimeout(300);
  return page.evaluate(() => {
    window.__openaideVirtualRowOverlap.active = false;
    return window.__openaideVirtualRowOverlap.maximum;
  });
}

async function planEntryLayout(plan) {
  return plan.locator(".agent-plan-entries").evaluate((list) => {
    const bounds = [...list.querySelectorAll(".agent-plan-entry-content")]
      .map((element) => element.getBoundingClientRect());
    return {
      clientHeight: list.clientHeight,
      clientWidth: list.clientWidth,
      maximumOverlap: bounds.slice(0, -1).reduce(
        (maximum, current, index) => Math.max(maximum, current.bottom - bounds[index + 1].top),
        0,
      ),
      scrollHeight: list.scrollHeight,
      scrollWidth: list.scrollWidth,
    };
  });
}

function expectPlanEntriesReadable(layout) {
  expect(layout.maximumOverlap).toBeLessThanOrEqual(0.5);
  expect(layout.scrollWidth).toBe(layout.clientWidth);
  expect(layout.scrollHeight).toBeGreaterThan(layout.clientHeight);
}

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
  await expect(page.getByRole("textbox", { name: "Message" }))
    .toHaveAttribute("contenteditable", "plaintext-only");
  await expect(page.getByLabel("Send message")).toBeDisabled();
}

async function send(page, text) {
  const editor = page.getByRole("textbox", { name: "Message" });
  await editor.fill(text);
  await page.getByLabel("Send message").click();
}

async function selectRenderedText(locator) {
  return locator.evaluate((element) => {
    const text = element.firstChild;
    if (!text?.textContent) throw new Error("The requested rendered text is missing.");
    const range = document.createRange();
    range.setStart(text, 0);
    range.setEnd(text, text.textContent.length);
    const selection = document.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
    const endRange = range.cloneRange();
    endRange.collapse(false);
    const rect = endRange.getBoundingClientRect();
    return { left: rect.left, top: rect.top };
  });
}

function composerText(element) {
  // ComposerEditor adds one terminal <br> solely to keep a final empty line
  // visible and placeable. It is not part of the logical draft text.
  return element.innerText.endsWith("\n") ? element.innerText.slice(0, -1) : element.innerText;
}

async function startComposerConnectionTrace(page) {
  await page.evaluate(() => {
    const expected = ["ready", "reconnecting", "ready"];
    const transitions = [];
    const sample = () => {
      const status = document.querySelector(".composer-footer-status");
      const value = status?.textContent?.includes("Reconnecting") ? "reconnecting" : "ready";
      if (transitions.at(-1) !== value) transitions.push(value);
      const completed = expected.every((item, index) => transitions[index] === item);
      if (completed) {
        observer.disconnect();
        window.__openaideComposerConnectionTrace.completed = true;
      }
    };
    const observer = new MutationObserver(sample);
    window.__openaideComposerConnectionTrace = { completed: false, transitions };
    sample();
    if (!window.__openaideComposerConnectionTrace.completed) {
      observer.observe(document.body, {
        attributes: true,
        childList: true,
        subtree: true,
      });
    }
  });
}

async function waitForComposerConnectionRecovery(page) {
  await page.waitForFunction(() => window.__openaideComposerConnectionTrace?.completed === true);
  return page.evaluate(() => window.__openaideComposerConnectionTrace?.transitions ?? []);
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
