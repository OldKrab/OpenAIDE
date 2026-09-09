import { readFile } from "node:fs/promises";
import { chromium } from "@playwright/test";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";
import { AgentMarkdown } from "./AgentMarkdown";

it("keeps portrait Markdown images proportional and inside Chat on desktop and phone", async () => {
  const css = await readFile(new URL("../styles/app/chat-messages.css", import.meta.url), "utf8");
  const html = renderToStaticMarkup(
    <AgentMarkdown className="chat-agent" onOpenImage={() => {}} text={"![Gesture navigation](https://images.test/gesture.svg)\n\n![Three-button navigation](https://images.test/buttons.svg)"} />,
  );
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  try {
    const page = await browser.newPage();
    await page.route("https://images.test/**", (route) => route.fulfill({
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="1080" height="2400"><rect width="1080" height="2400" fill="gray"/></svg>',
    }));
    for (const width of [1280, 390]) {
      await page.setViewportSize({ width, height: 844 });
      await page.setContent(`<style>${css}</style><main style="max-width:726px;margin:auto">${html}</main>`);
      await page.locator("img").evaluateAll((images) => Promise.all(images.map((image) => (image as HTMLImageElement).decode())));
      const bounds = await page.locator("img").evaluateAll((images) => images.map((image) => {
        const box = image.getBoundingClientRect();
        const parent = image.closest("main")!.getBoundingClientRect();
        return { width: box.width, height: box.height, right: box.right, parentRight: parent.right, parentWidth: parent.width };
      }));
      expect(bounds).toHaveLength(2);
      for (const box of bounds) {
        expect(box.width).toBeGreaterThan(0);
        expect(box.width).toBeLessThanOrEqual(box.parentWidth);
        expect(box.right).toBeLessThanOrEqual(box.parentRight);
        expect(box.height).toBeLessThan(844 / 2);
        expect(box.width / box.height).toBeCloseTo(1080 / 2400, 2);
      }
    }
  } finally {
    await browser.close();
  }
}, 20_000);
