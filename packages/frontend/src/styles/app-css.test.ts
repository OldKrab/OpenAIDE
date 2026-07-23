import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appCss = readCssWithImports("./app.css");
const tokensCss = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../../index.html", import.meta.url), "utf8");

function readCssWithImports(path: string): string {
  const url = new URL(path, import.meta.url);
  const css = readFileSync(url, "utf8");
  return css.replace(/@import\s+"([^"]+)";/g, (_match, importPath: string) =>
    readCssWithImports(importPath),
  );
}

describe("task list row styles", () => {
  it("renders Questions as one soft surface with quiet field separation and low-border controls", () => {
    expect(appCss).toMatch(/\.question-card\s*{[^}]*border:\s*0;[^}]*border-radius:\s*12px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-panel\) 78%, var\(--oa-bg\)\);/);
    expect(appCss).toMatch(/\.question-fields > \* \+ \*\s*{[^}]*border-top:\s*1px solid color-mix\(in oklch, var\(--oa-border\) 54%, transparent\);/);
    expect(appCss).toMatch(/\.question-choice-list label\s*{[^}]*border:\s*0;[^}]*border-radius:\s*8px;/);
    expect(appCss).toMatch(/\.question-value-field > input,[^{]+\.question-value-field > textarea,[^{]+\.question-choice-field > select\s*{[^}]*border:\s*0;[^}]*border-radius:\s*8px;/);
    expect(appCss).toMatch(/@media \(max-width:\s*470px\)[\s\S]*\.question-fields\s*{[^}]*margin-inline-start:\s*0;/);
  });

  it("uses the bundled Inter family for readable product and chat text", () => {
    expect(tokensCss).toMatch(/--oa-ui-font:\s*"Inter Variable",\s*system-ui,\s*sans-serif;/);
    expect(tokensCss).toMatch(/--oa-chat-font:\s*var\(--oa-ui-font\);/);
    expect(tokensCss).toMatch(/--oa-mono-font:\s*var\(--vscode-editor-font-family,/);
    expect(tokensCss).toMatch(/font-size:\s*14px;/);
    expect(tokensCss).not.toContain("var(--vscode-font-size");
  });

  it("uses one token-based scrollbar style across the webview", () => {
    expect(appCss).toMatch(/\*\s*{[^}]*scrollbar-width:\s*thin;[^}]*scrollbar-color:\s*color-mix\(in oklch, var\(--oa-muted\) 42%, transparent\) transparent;/);
    expect(appCss).toMatch(/\*::-webkit-scrollbar\s*{[^}]*width:\s*10px;[^}]*height:\s*10px;/);
    expect(appCss).toMatch(/\*::-webkit-scrollbar-track\s*{\s*background:\s*transparent;/);
    expect(appCss).toMatch(/\*::-webkit-scrollbar-thumb\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-muted\) 42%, transparent\);[^}]*background-clip:\s*content-box;/);
    expect(appCss).not.toMatch(/\.task-list::-webkit-scrollbar/);
    expect(appCss).not.toMatch(/\.composer-popover\s*{[^}]*scrollbar-color:/);
  });

  it("gives archive mode a restrained warm color identity", () => {
    expect(appCss).toMatch(/\.archive-sidebar\s*{\s*background:\s*color-mix\(in oklch, var\(--oa-warning\) 3%, var\(--oa-panel\)\);/);
    expect(appCss).toMatch(/\.archive-sidebar \.sidebar-actions button:hover,[^{]+{\s*background:\s*color-mix\(in oklch, var\(--oa-warning\) 7%, var\(--oa-raised\)\);/);
    expect(appCss).toMatch(/\.archive-section-head\s*{[^}]*display:\s*flex;[^}]*gap:\s*7px;/);
    expect(appCss).toMatch(/\.task-section-head \.archive-navigation\s*{[^}]*margin-left:\s*auto;[^}]*font-size:\s*12px;/);
    expect(appCss).not.toMatch(/\.task-mode-tabs\s*{/);
  });

  it("applies hover highlight to the whole task row, not only the open button", () => {
    expect(appCss).toMatch(/\.task-row:hover,\s*\.task-row:has\(:focus-visible\)\s*{\s*background:\s*var\(--oa-raised\);/);
    expect(appCss).not.toMatch(/\.task-row:hover,\s*\.task-row:focus-within\s*{/);
    expect(appCss).not.toContain(".task-row:hover .task-open");
  });

  it("overlays row actions on the trailing state slot like the approved prototype", () => {
    expect(appCss).toMatch(/\.task-row\s*{[^}]*position:\s*relative;[^}]*min-height:\s*32px;[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\);[^}]*margin-right:\s*0;/);
    expect(appCss).toMatch(/\.task-row\.selected\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-selected\) 36%, transparent\);[^}]*box-shadow:\s*none;/);
    expect(appCss).toMatch(/\.task-row-action\s*{[^}]*color:\s*var\(--oa-muted\);[^}]*opacity:\s*0;/);
    expect(appCss).toMatch(/\.task-row-action-slot\s*{[^}]*position:\s*absolute;[^}]*right:\s*0;[^}]*width:\s*32px;[^}]*pointer-events:\s*none;/);
    expect(appCss).toMatch(/\.task-row-body\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto;[^}]*align-items:\s*center;/);
    expect(appCss).not.toMatch(/\.task-row-time\s*{/);
    expect(appCss).toMatch(/\.task-row:hover \.task-row-action,\s*\.task-row:has\(:focus-visible\) \.task-row-action\s*{[^}]*opacity:\s*1;/);
    expect(appCss).not.toMatch(/\.task-row:hover \.task-row-action,\s*\.task-row:has\(:focus-visible\) \.task-row-action\s*{[^}]*color:/);
    expect(appCss).not.toMatch(/\.task-row:focus-within \.task-row-action/);
    expect(appCss).not.toMatch(/\.task-row\.selected \.task-row-action\s*{/);
    expect(appCss).toMatch(/\.task-row:has\(\.task-row-action:not\(:disabled\)\):hover \.task-trailing-meta,\s*\.task-row:has\(\.task-row-action:not\(:disabled\)\):has\(:focus-visible\) \.task-trailing-meta\s*{[^}]*visibility:\s*hidden;/);
    expect(appCss).not.toMatch(/\.task-row\.selected \.task-trailing-meta\s*{/);
    expect(appCss).toMatch(/\.task-row:hover \.task-row-action-slot,\s*\.task-row:has\(:focus-visible\) \.task-row-action-slot\s*{[^}]*pointer-events:\s*auto;/);
    expect(appCss).toMatch(/\.task-row-action\s*{[^}]*cursor:\s*pointer;/);
    expect(appCss).toMatch(/\.task-row-action:hover,\s*\.task-row-action:focus-visible\s*{[^}]*background:\s*transparent;[^}]*color:\s*var\(--oa-text\);/);
    expect(appCss).toMatch(/\.task-row-menu\s*{[^}]*position:\s*absolute;[^}]*min-width:\s*132px;[^}]*background:\s*var\(--oa-panel\);/);
    expect(appCss).not.toMatch(/\.task-row-action\s*{[^}]*pointer-events:\s*none;/);
  });

  it("lets task row context menus render outside their row bounds", () => {
    expect(appCss).toMatch(/\.task-row\s*{[^}]*overflow:\s*visible;/);
    expect(appCss).not.toMatch(/\.task-row\s*{[^}]*overflow:\s*hidden;/);
  });

  it("exposes Task details in the row menu only on mobile", () => {
    expect(appCss).toMatch(/button\.task-row-mobile-details-action\s*{\s*display:\s*none;/);
    expect(appCss).toMatch(/@media \(max-width:\s*760px\)\s*{[\s\S]*button\.task-row-mobile-details-action\s*{\s*display:\s*flex;/);
  });

  it("lets agent markdown use the full readable chat lane", () => {
    expect(appCss).toMatch(/\.chat-agent\s*{[^}]*width:\s*100%;[^}]*max-width:\s*100%;[^}]*min-width:\s*0;[^}]*font-family:\s*var\(--oa-chat-font\);[^}]*font-size:\s*15px;[^}]*line-height:\s*1\.5;/);
    expect(appCss).toMatch(/\.chat-agent a\s*{\s*color:\s*var\(--oa-focus\);/);
    expect(appCss).toMatch(/\.chat-agent code\s*{[^}]*font-family:\s*var\(--oa-mono-font\);/);
    expect(appCss).toMatch(/\.chat-agent :not\(pre\) > code\s*{[^}]*overflow-wrap:\s*anywhere;/);
    expect(appCss).toMatch(/\.chat-agent table\s*{[^}]*width:\s*100%;[^}]*table-layout:\s*fixed;/);
    expect(appCss).toMatch(/\.chat-agent th,\s*\.chat-agent td\s*{[^}]*overflow-wrap:\s*anywhere;/);
  });

  it("keeps authored messages distinct on the right side of the chat column", () => {
    expect(appCss).toMatch(/\.chat-user-block,\s*\.chat-user,\s*\.chat-system\s*{[^}]*font-size:\s*15px;[^}]*line-height:\s*1\.5;/);
    expect(appCss).toMatch(/\.chat-user-block\s*{[^}]*max-width:\s*min\(70ch, 82%\);[^}]*align-self:\s*flex-end;/);
    expect(appCss).not.toMatch(/\.chat-user-block\s*{[^}]*margin-inline-end:/);
    expect(appCss).toMatch(/\.chat-user-block\s*{[^}]*margin-block:\s*4px;/);
    expect(appCss).toMatch(/\.chat-user-block\s*{[^}]*justify-items:\s*end;/);
    expect(appCss).toMatch(/\.chat-user\s*{[^}]*border:\s*0;[^}]*border-radius:\s*10px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-focus\) 14%, var\(--oa-bg\)\);/);
    expect(appCss).toMatch(/\.chat-user\s*{[^}]*box-shadow:\s*none;[^}]*padding:\s*8px 11px;[^}]*overflow-wrap:\s*anywhere;/);
    expect(appCss).toMatch(/\.chat-user\s*{[^}]*white-space:\s*pre-wrap;/);
    expect(appCss).toMatch(/\.chat-attachment-list \+ \.chat-user\s*{\s*margin-top:\s*6px;/);
  });

  it("keeps copy actions in a compact row below their messages", () => {
    expect(appCss).toMatch(/\.chat-agent-block\s*{[^}]*width:\s*100%;[^}]*display:\s*grid;[^}]*gap:\s*0;/);
    expect(appCss).not.toMatch(/\.message-list > \.chat-thought-block,[^{]+\.message-list > \.activity-group\s*{[^}]*margin-inline-start:/);
    expect(appCss).toMatch(/\.chat-user-block\s*{[^}]*display:\s*grid;[^}]*gap:\s*0;/);
    expect(appCss).toMatch(/\.chat-message-actions\s*{[^}]*min-height:\s*16px;[^}]*opacity:\s*0;/);
    expect(appCss).not.toMatch(/\.chat-message-actions\s*{[^}]*position:\s*absolute;/);
    expect(appCss).toMatch(/\.chat-agent-block:hover \.chat-message-actions,\s*\.chat-agent-block:focus-within \.chat-message-actions,\s*\.chat-user-block:hover \.chat-message-actions,\s*\.chat-user-block:focus-within \.chat-message-actions\s*{\s*opacity:\s*1;/);
    expect(appCss).toMatch(/\.chat-message-action\s*{[^}]*width:\s*20px;[^}]*min-height:\s*16px;[^}]*border:\s*0;[^}]*background:\s*transparent;/);
    expect(appCss).toMatch(/\.message-list > \.activity-group\s*{[^}]*margin-block-end:\s*4px;/);
  });

  it("keeps narrow transcript controls touchable", () => {
    expect(appCss).toMatch(/\.chat-message-actions\s*{[^}]*min-height:\s*28px;/);
    expect(appCss).toMatch(/\.chat-message-action\s*{[^}]*width:\s*36px;[^}]*min-height:\s*36px;/);
    expect(appCss).toMatch(/\.activity-group > \.activity-disclosure-trigger,[^{]+\.chat-thought-block > \.activity-disclosure-trigger\s*{[^}]*min-height:\s*36px;/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.chat-agent-block,[^{]+\.message-list > \.chat-thought-block,[^{]+\.message-list > \.activity-group\s*{[^}]*margin-inline-start:/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.chat-user-block\s*{[^}]*margin-inline-end:/);
    expect(appCss).toMatch(/\.chat-agent,[^{]+\.chat-user,[^{]+\.chat-system\s*{[^}]*font-size:\s*15px;[^}]*line-height:\s*1\.5;/);
  });

  it("keeps the task composer docked while only chat history scrolls", () => {
    expect(appCss).toMatch(/\.task-surface\s*{[^}]*min-height:\s*0;[^}]*height:\s*100%;[^}]*overflow:\s*hidden;/);
    expect(appCss).toMatch(/\.web-workbench-shell \.sidebar\s*{[^}]*grid-column:\s*1;[^}]*grid-row:\s*1;/);
    expect(appCss).toMatch(/\.web-main-surface\s*{[^}]*grid-column:\s*2;[^}]*grid-row:\s*1;/);
    expect(appCss).toMatch(/\.chat-column\s*{[^}]*grid-template-rows:\s*minmax\(0,\s*1fr\) auto;/);
    expect(appCss).toMatch(/\.message-list\s*{[^}]*width:\s*100%;[^}]*min-height:\s*0;[^}]*overflow:\s*auto;/);
    expect(appCss).toMatch(/\.message-list\s*{[^}]*gap:\s*8px;/);
    expect(appCss).toMatch(/\.message-list\s*{[^}]*padding-inline:\s*max\(0px,\s*calc\(\(100% - 760px\) \/ 2\)\);/);
    expect(appCss).toMatch(/\.message-list\s*{[^}]*padding-bottom:\s*64px;/);
    expect(appCss).toMatch(/@media \(max-width:\s*760px\)\s*{[^}]*\.message-list\s*{[^}]*padding-bottom:\s*36px;/);
  });

  it("shows working status as a quiet animated row in the message stream", () => {
    expect(appCss).toMatch(/\.working-status\s*{[^}]*margin-top:\s*32px;/);
    expect(appCss).toMatch(/\.message-list > \.activity-group \+ \.working-status\s*{[^}]*margin-top:\s*28px;/);
    expect(appCss).toMatch(/\.working-status\s*{[^}]*color:\s*var\(--oa-muted\);[^}]*display:\s*inline-flex;/);
    expect(appCss).toMatch(/\.working-status-dots span\s*{[^}]*animation:\s*working-dot-pulse 1\.2s ease-in-out infinite;/);
    expect(appCss).toMatch(/\.working-status-duration-separator\s*{[^}]*width:\s*1px;[^}]*height:\s*12px;[^}]*background:\s*var\(--oa-border\);/);
    expect(appCss).toMatch(/\.working-status-duration\s*{[^}]*font-variant-numeric:\s*tabular-nums;[^}]*white-space:\s*nowrap;/);
    expect(appCss).toMatch(/@media \(prefers-reduced-motion:\s*reduce\)\s*{[^}]*\.working-status-dots span,\s*\.composer-submit-pending svg\s*{\s*animation:\s*none;/);
  });

  it("centers task loading states in the full task surface", () => {
    expect(appCss).toMatch(/\.task-loading\s*{[^}]*grid-template-rows:\s*auto;[^}]*place-items:\s*center;[^}]*align-content:\s*center;/);
    expect(appCss).toMatch(/\.task-loading-status\s*{[^}]*display:\s*inline-flex;[^}]*gap:\s*8px;/);
  });

  it("shows task title, status, and agent identity in the opened task header", () => {
    expect(appCss).toMatch(/\.task-header\s*{[^}]*min-width:\s*0;[^}]*display:\s*grid;[^}]*grid-template-columns:\s*minmax\(0, 1fr\) auto;/);
    expect(appCss).toMatch(/\.task-header-title\s*{[^}]*display:\s*inline-flex;[^}]*gap:\s*8px;/);
    expect(appCss).toMatch(/\.task-header-title strong\s*{[^}]*flex:\s*0 1 auto;[^}]*max-width:\s*72ch;[^}]*text-overflow:\s*ellipsis;/);
    expect(appCss).not.toMatch(/\.task-header-title strong\s*{[^}]*max-width:\s*min\(48ch, 55vw\);/);
    expect(appCss).toMatch(/\.task-header-status\s*{[^}]*display:\s*inline-flex;[^}]*font-size:\s*12px;/);
    expect(appCss).toMatch(/\.task-header-agent\s*{[^}]*display:\s*inline-flex;[^}]*font-size:\s*12px;/);
  });

  it("uses an explicit disclosure affordance for activity groups", () => {
    expect(appCss).toMatch(/\.activity-group summary,\s*\.activity-group > \.activity-disclosure-trigger\s*{[^}]*cursor:\s*pointer;[^}]*list-style:\s*none;/);
    expect(appCss).toMatch(/\.activity-group summary::-webkit-details-marker\s*{\s*display:\s*none;\s*}/);
    expect(appCss).toMatch(/\.activity-disclosure-icon\s*{[^}]*transition:\s*transform 180ms cubic-bezier\(0\.22, 1, 0\.36, 1\);/);
    expect(appCss).toMatch(/\.activity-group\[open\] \.activity-disclosure-icon,\s*\.activity-group\.open \.activity-disclosure-icon\s*{\s*transform:\s*rotate\(90deg\);/);
  });

  it("animates activity disclosure content with reduced-motion support", () => {
    expect(appCss).toMatch(/\.activity-disclosure-body\s*{[^}]*grid-template-rows:\s*0fr;[^}]*transition:\s*grid-template-rows 210ms cubic-bezier\(0\.22, 1, 0\.36, 1\);/);
    expect(appCss).toMatch(/\.activity-disclosure-body\.open\s*{\s*grid-template-rows:\s*1fr;/);
    expect(appCss).toMatch(/\.activity-disclosure-content\s*{[^}]*min-height:\s*0;[^}]*contain:\s*layout paint;[^}]*overflow:\s*hidden;/);
    expect(appCss).not.toMatch(/::details-content\s*{[^}]*opacity:/);
    expect(appCss).toMatch(/@media \(prefers-reduced-motion:\s*reduce\)\s*{[^}]*\.activity-disclosure-body\s*{[^}]*transition-duration:\s*0\.01ms;/);
  });

  it("renders thought messages separately from activity groups", () => {
    expect(appCss).toMatch(/\.chat-thought-block\s*{[^}]*display:\s*grid;[^}]*gap:\s*3px;/);
    expect(appCss).toMatch(/\.chat-thought-block > summary,\s*\.chat-thought-block > \.activity-disclosure-trigger\s*{[^}]*cursor:\s*pointer;[^}]*list-style:\s*none;/);
    expect(appCss).toMatch(/\.chat-thought-block\[open\] \.chat-thought-disclosure,\s*\.chat-thought-block\.open \.chat-thought-disclosure\s*{\s*transform:\s*rotate\(90deg\);/);
    expect(appCss).toMatch(/\.chat-thought\s*{[^}]*border-left:\s*1px solid color-mix\(in oklch, var\(--oa-muted\) 45%, transparent\);/);
  });

  it("supports expandable per-tool activity details", () => {
    expect(appCss).toMatch(/\.activity-step > summary,\s*\.activity-step > \.activity-disclosure-trigger\s*{[^}]*display:\s*grid;[^}]*cursor:\s*pointer;[^}]*list-style:\s*none;/);
    expect(appCss).toMatch(/\.activity-step\[open\] \.activity-step-disclosure,\s*\.activity-step\.open \.activity-step-disclosure\s*{\s*transform:\s*rotate\(90deg\);/);
    expect(appCss).toMatch(/\.activity-tool-details\s*{[^}]*--activity-tool-data-max-height:\s*min\(340px, 46vh\);[^}]*max-width:\s*100%;[^}]*display:\s*grid;[^}]*gap:\s*6px;[^}]*margin:\s*4px 0 2px 18px;/);
    expect(appCss).toMatch(/\.activity-step\.tool-edit \.activity-kind-icon\s*{\s*color:\s*color-mix\(in oklch, var\(--oa-warning\) 70%, var\(--oa-muted\)\);/);
    expect(appCss).toMatch(/\.activity-tool-meta\s*{[^}]*display:\s*flex;[^}]*flex-wrap:\s*wrap;[^}]*list-style:\s*none;/);
    expect(appCss).toMatch(/\.activity-tool-skeleton\s*{[^}]*display:\s*grid;[^}]*gap:\s*5px;/);
    expect(appCss).toMatch(/\.activity-tool-skeleton span\s*{[^}]*height:\s*8px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-muted\) 18%, transparent\);/);
    expect(appCss).toMatch(/\.activity-tool-command\s*{[^}]*width:\s*fit-content;[^}]*max-width:\s*100%;[^}]*max-height:\s*var\(--activity-tool-data-max-height\);[^}]*overflow:\s*auto;/);
    expect(appCss).toMatch(/\.activity-tool-code\s*{[^}]*max-height:\s*var\(--activity-tool-data-max-height\);[^}]*overflow:\s*auto;/);
    expect(appCss).toMatch(/\.activity-tool-fields\s*{[^}]*grid-template-columns:\s*max-content minmax\(0, 1fr\);/);
    expect(appCss).toMatch(/\.activity-tool-inline-fields\s*{[^}]*display:\s*flex;[^}]*flex-wrap:\s*wrap;/);
    expect(appCss).toMatch(/\.activity-tool-path-link\s*{[^}]*font-family:\s*var\(--oa-mono-font\);[^}]*cursor:\s*pointer;/);
    expect(appCss).toMatch(/\.activity-tool-diff-line\.add\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-success\) 13%, transparent\);/);
    expect(appCss).toMatch(/\.activity-tool-diff-line\.remove\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-danger\) 11%, transparent\);/);
    expect(appCss).toMatch(/\.activity-search-results li\s*{[^}]*grid-template-columns:\s*minmax\(0, max-content\) minmax\(0, 1fr\);/);
  });

  it("renders edit details as compact editor-style diff hunks", () => {
    expect(appCss).toMatch(/\.activity-step pre\.edit-tool-diff\s*{[^}]*max-height:\s*min\(360px, 72vh\);[^}]*border:\s*0;[^}]*border-radius:\s*0;[^}]*background:\s*transparent;[^}]*padding:\s*0;/);
    expect(appCss).toMatch(/\.edit-tool-line,[^{]+\.edit-tool-hunk-row,[^{]+\.edit-tool-omitted-row\s*{[^}]*grid-template-columns:\s*38px 38px 16px minmax\(max-content, 1fr\);/);
    expect(appCss).toMatch(/\.edit-tool-new-line-number\s*{[^}]*border-right:\s*1px solid color-mix\(in oklch, var\(--oa-border\) 48%, transparent\);/);
    expect(appCss).not.toMatch(/\.edit-tool-old-line-number,[^{]+\.edit-tool-new-line-number\s*{[^}]*border-right:/);
    expect(appCss).toMatch(/\.edit-tool-hunk-row\s*{[^}]*background:\s*transparent;[^}]*color:\s*color-mix\(in oklch, var\(--oa-muted\) 88%, var\(--oa-text\)\);/);
    expect(appCss).toMatch(/\.edit-tool-omitted-row::before\s*{[^}]*height:\s*1px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-border\) 46%, transparent\);/);
    expect(appCss).toMatch(/\.edit-tool-line\.add\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-success\) 7%, transparent\);/);
    expect(appCss).toMatch(/\.edit-tool-line\.remove\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-danger\) 6%, transparent\);/);
  });

  it("uses one geometry for every grouped activity row", () => {
    expect(appCss).toMatch(
      /\.activity-step > summary,\s*\.activity-step > \.activity-disclosure-trigger\s*{[^}]*align-items:\s*center;[^}]*min-height:\s*26px;/,
    );
    expect(appCss).toMatch(/\.activity-step \.activity-step-title\s*{[^}]*font-size:\s*13px;[^}]*font-weight:\s*400;/);
    expect(appCss).toMatch(
      /\.activity-step-disclosure-placeholder\s*{[^}]*width:\s*12px;[^}]*height:\s*12px;[^}]*flex:\s*0 0 auto;/,
    );
  });

  it("keeps command actions readable while trimming normal-weight monospace commands", () => {
    expect(appCss).toMatch(
      /\.activity-step-title\.command\s*{[^}]*display:\s*inline-flex;[^}]*align-items:\s*baseline;[^}]*gap:\s*5px;/,
    );
    expect(appCss).toMatch(/\.activity-step-action\s*{[^}]*flex:\s*0 0 auto;/);
    expect(appCss).toMatch(
      /\.activity-step-command\s*{[^}]*min-width:\s*0;[^}]*overflow:\s*hidden;[^}]*text-overflow:\s*ellipsis;[^}]*white-space:\s*nowrap;[^}]*font-family:\s*var\(--oa-mono-font\);[^}]*font-weight:\s*400;/,
    );
    expect(appCss).toMatch(
      /\.activity-step-command\s*{[^}]*border-radius:\s*3px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-muted\) 9%, transparent\);[^}]*padding:\s*1px 4px;/,
    );
  });

  it("contains long execute tool output inside the chat column", () => {
    expect(appCss).toMatch(/\.activity-group\s*{[^}]*max-width:\s*100%;/);
    expect(appCss).toMatch(/\.activity-step-list\s*{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;/);
    expect(appCss).toMatch(/\.activity-tool-execute-detail\s*{[^}]*max-width:\s*100%;[^}]*overflow-x:\s*hidden;/);
    expect(appCss).toMatch(/\.execute-command-chip\s*{[^}]*display:\s*block;[^}]*overflow-x:\s*auto;/);
    expect(appCss).toMatch(/\.execute-output\s*{[^}]*max-width:\s*100%;[^}]*overflow-x:\s*hidden;/);
    expect(appCss).toMatch(/\.execute-output pre\s*{[^}]*max-width:\s*100%;[^}]*overflow:\s*auto;/);
  });

  it("keeps execute activity icons inline instead of filled badges", () => {
    expect(appCss).toMatch(
      /\.activity-step \.activity-kind-icon\s*{[^}]*width:\s*12px;[^}]*height:\s*12px;[^}]*flex:\s*0 0 12px;/,
    );
    expect(appCss).toMatch(/\.activity-step\.tool-execute \.activity-kind-icon\s*{\s*color:\s*color-mix\(in oklch, var\(--oa-muted\) 82%, var\(--oa-text\)\);/);
    expect(appCss).not.toMatch(/\.activity-step\.tool-execute \.activity-kind-icon\s*{[^}]*background:/);
    expect(appCss).not.toMatch(/\.activity-step\.tool-execute \.activity-kind-icon\s*{[^}]*width:\s*22px;/);
  });

  it("gives permission cards tool-kind icon hooks", () => {
    expect(appCss).toMatch(/\.permission-card\s*{[^}]*border:\s*1px solid color-mix\(in oklch, var\(--oa-warning\) 34%, var\(--oa-border\)\);[^}]*background:\s*color-mix\(in oklch, var\(--oa-warning\) 5%, var\(--oa-panel\)\);[^}]*margin:\s*8px 0;/);
    expect(appCss).toMatch(/\.permission-card\.resolved\s*{[^}]*border-color:\s*color-mix\(in oklch, var\(--oa-border\) 72%, transparent\);[^}]*background:\s*color-mix\(in oklch, var\(--oa-raised\) 24%, transparent\);/);
    expect(appCss).toMatch(/\.permission-body\s*{[^}]*max-width:\s*100%;[^}]*overflow-x:\s*hidden;[^}]*padding-left:\s*26px;/);
    expect(appCss).toMatch(/\.permission-card\.tool-read \.permission-icon\s*{\s*color:\s*color-mix\(in oklch, var\(--oa-focus\) 70%, var\(--oa-muted\)\);/);
    expect(appCss).toMatch(/\.permission-card\.tool-edit \.permission-icon\s*{\s*color:\s*color-mix\(in oklch, var\(--oa-warning\) 88%, var\(--oa-text\)\);/);
    expect(appCss).toMatch(/\.permission-card\.tool-execute \.permission-icon\s*{\s*color:\s*color-mix\(in oklch, var\(--oa-warning\) 88%, var\(--oa-text\)\);/);
    expect(appCss).not.toMatch(/\.permission-card\.tool-execute \.permission-icon\s*{[^}]*background:/);
  });

  it("shares new-task and composer geometry across app shells", () => {
    expect(appCss).toMatch(/\.composer\[data-keyboard-focus="true"\]:focus-within\s*{\s*border-color:\s*color-mix\(in oklch, var\(--oa-focus\) 42%, var\(--oa-border\)\);/);
    expect(appCss).not.toMatch(/\.composer:focus-within\s*{/);
    expect(appCss).toMatch(/\.composer textarea:focus-visible,\s*\.composer-editor:focus-visible\s*{\s*outline:\s*0;/);
    expect(appCss).toMatch(/\.composer\s*{[^}]*width:\s*min\(760px, 100%\);[^}]*border-radius:\s*20px;[^}]*box-shadow:\s*none;/);
    expect(appCss).toMatch(/\.new-task-context-controls\s*{[^}]*width:\s*fit-content;[^}]*border-radius:\s*999px;/);
    expect(appCss).toMatch(/\.composer-send-button\s*{[^}]*width:\s*32px;[^}]*height:\s*32px;[^}]*border-radius:\s*999px;/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.composer\s*{[^}]*width:\s*min\(760px, 100%\)/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.new-task-context-controls\s*{[^}]*width:\s*fit-content/);
    expect(appCss).toMatch(/\.task-surface:not\(\.new-task-surface\) \.composer\s*{\s*max-height:\s*50dvh;/);
    expect(appCss).toMatch(/\.composer textarea,\s*\.composer-editor\s*{[^}]*min-height:\s*40px;[^}]*max-height:\s*calc\(50dvh - 72px\);/);
    expect(appCss).toMatch(/\.new-task-surface \.composer textarea,\s*\.new-task-surface \.composer-editor\s*{\s*max-height:\s*min\(480px,\s*65dvh\);/);
  });

  it("keeps shared composer send and stop hover states specific", () => {
    expect(appCss).toMatch(/\.composer-send-button:not\(:disabled\)\s*{[^}]*background:\s*var\(--vscode-button-background, var\(--oa-focus\)\);[^}]*color:\s*var\(--vscode-button-foreground, oklch\(0\.97 0\.008 250\)\);/);
    expect(appCss).toMatch(/\.composer-send-button:hover:not\(:disabled\)\s*{\s*background:\s*var\(--vscode-button-hoverBackground, color-mix\(in oklch, var\(--oa-focus\) 82%, var\(--oa-text\)\)\);/);
    expect(appCss).toMatch(/\.composer-stop-button:not\(:disabled\)\s*{[^}]*background:\s*transparent;/);
    expect(appCss).toMatch(/\.composer-stop-button:hover:not\(:disabled\)\s*{\s*background:\s*color-mix\(in oklch, var\(--oa-danger\) 12%, transparent\);/);
  });

  it("centers composer action button icons in a stable square hit target", () => {
    expect(appCss).toMatch(/\.composer-actions \.composer-icon-button:not\(\.composer-send-button\)\s*{[^}]*min-width:\s*30px;[^}]*min-height:\s*30px;[^}]*width:\s*30px;[^}]*height:\s*30px;[^}]*display:\s*grid;[^}]*place-items:\s*center;[^}]*line-height:\s*0;/);
    expect(appCss).toMatch(/\.composer-actions \.composer-icon-button:not\(\.composer-send-button\) svg\s*{\s*display:\s*block;/);
  });

  it("gives the shared new-task screen a restrained but clear visual hierarchy", () => {
    expect(appCss).toMatch(/\.new-task-center\s*{[^}]*align-content:\s*safe center;[^}]*gap:\s*12px;[^}]*padding:\s*clamp\(12px,\s*3vh,\s*24px\) clamp\(18px,\s*8vw,\s*84px\) clamp\(16px,\s*6vh,\s*42px\);/);
    expect(appCss).toMatch(/\.new-task-center h1\s*{[^}]*font-size:\s*20px;[^}]*font-weight:\s*500;/);
  });

  it("keeps the mobile new-task form visible when the software keyboard reduces the viewport", () => {
    expect(indexHtml).toMatch(/name="viewport" content="[^"]*interactive-widget=resizes-content[^"]*"/);
    expect(appCss).toMatch(/\.new-task-center\s*{[^}]*align-content:\s*start;[^}]*padding:\s*clamp\(28px,\s*16dvh,\s*112px\) 12px 12px;/);
  });

  it("opens the mobile new-task slash menu below the composer instead of behind the header", () => {
    expect(appCss).toMatch(/\.new-task-surface \.composer-slash-popover\s*{[^}]*bottom:\s*auto;[^}]*top:\s*calc\(100% \+ 6px\);/);
  });

  it("keeps new-task context menus from resizing the composer layout", () => {
    expect(appCss).toMatch(/\.new-task-context-anchor\s*{[^}]*position:\s*static;/);
    expect(appCss).not.toMatch(/\.new-task-context-anchor\.context-menu-open\s*{[^}]*grid-row:\s*span 2;/);
    expect(appCss).toMatch(/\.new-task-context-menu\s*{[^}]*position:\s*absolute;[^}]*top:\s*calc\(100% \+ 6px\);/);
    expect(appCss).toMatch(/@media \(max-height:\s*320px\)\s*{[^}]*\.new-task-center\s*{[^}]*gap:\s*6px;[^}]*padding-block:\s*12px;/);
    expect(appCss).toMatch(/\.composer textarea,\s*\.composer-editor\s*{[^}]*min-height:\s*40px;[^}]*max-height:\s*80px;/);
  });

  it("renders new-task context selectors as one quiet control group", () => {
    expect(appCss).toMatch(/\.new-task-context-controls\s*{[^}]*width:\s*fit-content;[^}]*position:\s*relative;[^}]*border:\s*1px solid color-mix\(in oklch, var\(--oa-border\) 48%, transparent\);[^}]*border-radius:\s*999px;[^}]*background:\s*color-mix\(in oklch, var\(--oa-panel\) 72%, transparent\);/);
    expect(appCss).toMatch(/\.new-task-context-anchor\s*{[^}]*position:\s*static;/);
    expect(appCss).toMatch(/\.new-task-context-controls \.composer-pill\s*{[^}]*min-height:\s*30px;[^}]*border-radius:\s*999px;[^}]*padding-inline:\s*9px;/);
    expect(appCss).toMatch(/\.new-task-context-controls \.composer-pill\[aria-expanded="true"\]\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-text\) 9%, transparent\);[^}]*color:\s*var\(--oa-text\);/);
  });

  it("keeps all mobile task context labels on one row while workspace absorbs overflow", () => {
    expect(appCss).toMatch(/@media \(max-width:\s*430px\)[\s\S]*?\.new-task-context-controls\s*{[^}]*grid-template-columns:\s*fit-content\(36%\) minmax\(0,\s*1fr\) max-content;[^}]*align-items:\s*center;/);
    expect(appCss).toMatch(/\.new-task-context-anchor-workspace \.composer-pill\s*{[^}]*width:\s*100%;/);
    expect(appCss).not.toMatch(/\.new-task-agent-selector \.composer-pill-label,[^{]+\.new-task-agent-selector > svg:last-child\s*{[^}]*display:\s*none;/);
    expect(appCss).not.toMatch(/grid-template-areas:\s*\n\s*"project agent"\s*\n\s*"workspace workspace"/);
  });

  it("gives project and agent menus enough room for readable option details", () => {
    expect(appCss).toMatch(/\.new-task-context-menu\s*{[^}]*width:\s*min\(320px,\s*calc\(100vw - 32px\)\);[^}]*border-radius:\s*10px;[^}]*box-shadow:\s*0 12px 30px color-mix\(in oklch, var\(--oa-text\) 12%, transparent\);/);
    expect(appCss).toMatch(/\.new-task-context-anchor-project \.new-task-context-menu\s*{[^}]*width:\s*min\(360px,\s*calc\(100vw - 32px\)\);/);
    expect(appCss).toMatch(/\.new-task-context-menu > button\s*{[^}]*min-height:\s*42px;[^}]*grid-template-columns:\s*18px minmax\(0,\s*1fr\) 16px;[^}]*padding:\s*7px 9px;/);
    expect(appCss).toMatch(/\.new-task-context-menu > button small\s*{[^}]*font-size:\s*12px;[^}]*line-height:\s*1\.3;/);
    expect(appCss).toMatch(/\.new-task-context-menu > button\[aria-checked="true"\]\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-text\) 5%, transparent\);/);
    expect(appCss).toMatch(/\.composer-menu-selection\s*{[^}]*display:\s*grid;[^}]*place-items:\s*center;/);
    expect(appCss).toMatch(/\.new-task-context-anchor-project \.new-task-context-menu > \.composer-menu-choice-compact\s*{[^}]*min-height:\s*36px;[^}]*align-items:\s*center;/);
    expect(appCss).toMatch(/\.new-task-context-anchor-project \.new-task-context-menu,[^{]+\.new-task-context-anchor-agent \.new-task-context-menu\s*{[^}]*left:\s*50%;[^}]*right:\s*auto;[^}]*transform:\s*translateX\(-50%\);/);
  });

  it("separates project choices, folder browsing, and manual workspace entry", () => {
    expect(appCss).toMatch(/\.new-task-context-menu-heading\s*{[^}]*font-size:\s*11px;[^}]*padding:\s*4px 9px 3px;/);
    expect(appCss).toMatch(/\.new-workspace-picker-row,[^{]+\.new-workspace-picker-status button\s*{[^}]*min-height:\s*34px;[^}]*padding:\s*6px 8px;/);
    expect(appCss).toMatch(/\.new-workspace-entry-row\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto;/);
    expect(appCss).toMatch(/\.new-workspace-entry-row button\s*{[^}]*min-width:\s*62px;[^}]*display:\s*inline-flex;[^}]*gap:\s*6px;/);
  });

  it("opens composer popovers outside the composer so file lists do not cover the textarea", () => {
    expect(appCss).toMatch(/\.composer-menu-anchor\s*{\s*display:\s*contents;/);
    expect(appCss).toMatch(/\.composer-popover\s*{[^}]*bottom:\s*calc\(100% \+ 6px\);/);
    expect(appCss).toMatch(/\.new-task-surface \.composer-popover\s*{[^}]*top:\s*calc\(100% \+ 6px\);[^}]*bottom:\s*auto;/);
    expect(appCss).not.toMatch(/\.composer-menu-anchor\s*{\s*position:\s*relative;/);
    expect(appCss).not.toMatch(/\.composer-popover\s*{[^}]*bottom:\s*42px;/);
  });

  it("keeps composer file browser popovers inside short desktop viewports", () => {
    expect(appCss).toMatch(/\.composer-file-browser-popover\s*{[^}]*max-height:\s*min\(320px,\s*calc\(100dvh - 24px\)\);/);
  });

  it("wraps composer menu descriptions so warning copy remains readable", () => {
    expect(appCss).toMatch(/\.composer-popover small\s*{[^}]*white-space:\s*normal;[^}]*overflow-wrap:\s*anywhere;/);
  });

  it("styles nested composer menu back navigation separately from option values", () => {
    expect(appCss).toMatch(/\.composer-popover \.composer-popover-back\s*{[^}]*border-bottom:\s*1px solid color-mix\(in oklch, var\(--oa-border\) 72%, transparent\);[^}]*color:\s*var\(--oa-muted\);/);
    expect(appCss).toMatch(/\.composer-popover \.composer-popover-back span\s*{[^}]*font-size:\s*12px;[^}]*font-weight:\s*600;/);
  });

  it("leaves desktop composer option pills wide enough for reasoning labels", () => {
    expect(appCss).toMatch(/\.composer-pill\s*{\s*max-width:\s*220px;/);
  });

  it("shows project task counts in grouped sidebar headers", () => {
    expect(appCss).toMatch(/\.project-task-group-header\s*{[^}]*margin-bottom:\s*4px;/);
    expect(appCss).toMatch(/\.project-task-group-toggle\s*{[^}]*color:\s*var\(--oa-text\);/);
    expect(appCss).toMatch(/\.project-task-group-toggle strong\s*{[^}]*color:\s*var\(--oa-text\);[^}]*font-size:\s*13px;[^}]*font-weight:\s*600;/);
    expect(appCss).toMatch(/\.project-task-group-counts\s*{[^}]*font-size:\s*11px;[^}]*color:\s*var\(--oa-muted\);/);
    expect(appCss).toMatch(/\.project-task-more\s*{[^}]*min-height:\s*32px;[^}]*background:\s*transparent;[^}]*justify-self:\s*stretch;[^}]*font-size:\s*13px;/);
    expect(appCss).toMatch(/\.project-task-group-toggle span\s*{[^}]*display:\s*grid;/);
  });

  it("keeps settings header and tabs fixed while settings content scrolls", () => {
    expect(appCss).toMatch(/\.app-shell\s*{[^}]*height:\s*100vh;[^}]*overflow:\s*hidden;/);
    expect(appCss).toMatch(/\.settings-view\s*{[^}]*height:\s*100vh;[^}]*overflow:\s*hidden;[^}]*grid-template-rows:\s*auto minmax\(0,\s*1fr\);/);
    expect(appCss).toMatch(/\.settings-body\s*{[^}]*min-height:\s*0;[^}]*align-items:\s*stretch;[^}]*overflow:\s*hidden;/);
    expect(appCss).toMatch(/\.settings-tabs\s*{[^}]*align-self:\s*start;/);
    expect(appCss).toMatch(/\.settings-tabs button\s*{[^}]*align-self:\s*start;/);
    expect(appCss).toMatch(/\.settings-content\s*{[^}]*min-height:\s*0;[^}]*overflow-x:\s*hidden;[^}]*overflow-y:\s*auto;[^}]*padding-bottom:\s*42px;/);
    expect(appCss).toMatch(/\.settings-tab-panel\s*{[^}]*min-width:\s*0;[^}]*min-height:\s*0;/);
  });

  it("uses compact editor-native proportions for Agent settings", () => {
    expect(appCss).toMatch(/\.settings-body\s*{[^}]*grid-template-columns:\s*160px minmax\(0,\s*1fr\);[^}]*gap:\s*16px;/);
    expect(appCss).toMatch(/\.agent-settings-layout\s*{[^}]*grid-template-columns:\s*minmax\(260px,\s*320px\) minmax\(380px,\s*1fr\);/);
    expect(appCss).toMatch(/\.agent-settings-list button\s*{[^}]*min-height:\s*48px;[^}]*border-radius:\s*var\(--oa-radius-sm\);/);
    expect(appCss).toMatch(/\.agent-detail-identity\s*{[^}]*grid-template-columns:\s*48px minmax\(0,\s*1fr\);/);
    expect(appCss).toMatch(/\.agent-detail-avatar\s*{[^}]*width:\s*48px;[^}]*height:\s*48px;/);
    expect(appCss).toMatch(/\.agent-status-panel\s*{[^}]*border-radius:\s*0;[^}]*background:\s*transparent;[^}]*padding:\s*10px 0;/);
  });

  it("keeps mobile settings tabs adjacent to their panel", () => {
    expect(appCss).toMatch(/\.settings-body\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\);[^}]*grid-template-rows:\s*auto minmax\(0,\s*1fr\);[^}]*gap:\s*12px;/);
  });

  it("keeps mobile Add Agent controls at touch target size", () => {
    expect(appCss).toMatch(/body\[data-shell="web"\] \.agent-field input,[^{]+\.agent-env-row input\s*{[^}]*min-height:\s*44px;[^}]*font-size:\s*16px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.agent-icon-picker button\s*{[^}]*width:\s*44px;[^}]*height:\s*44px;[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.agent-enabled-toggle\s*{[^}]*min-height:\s*44px;/);
  });

  it("renders general settings as a searchable compact row list", () => {
    expect(appCss).toMatch(/\.settings-filter\s*{[^}]*border:\s*1px solid var\(--oa-border\);[^}]*display:\s*flex;/);
    expect(appCss).toMatch(/\.settings-filter:focus-within\s*{\s*border-color:\s*var\(--oa-focus\);/);
    expect(appCss).toMatch(/\.settings-common-list\s*{[^}]*display:\s*grid;[^}]*gap:\s*12px;/);
    expect(appCss).toMatch(/\.settings-section\s*{[^}]*border-top:\s*1px solid var\(--oa-border\);[^}]*display:\s*grid;/);
    expect(appCss).toMatch(/\.settings-row\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) minmax\(140px,\s*max-content\);/);
    expect(appCss).toMatch(/\.settings-row-value\s*{[^}]*justify-self:\s*end;[^}]*text-overflow:\s*ellipsis;/);
    expect(appCss).toMatch(/code\.settings-row-value\s*{[^}]*overflow-x:\s*auto;[^}]*text-overflow:\s*clip;/);
    expect(appCss).toMatch(/\.settings-switch-track\s*{[^}]*pointer-events:\s*none;/);
    expect(appCss).toMatch(/\.settings-switch input:checked \+ \.settings-switch-track\s*{[^}]*background:\s*color-mix\(in oklch, var\(--oa-focus\) 18%, var\(--oa-raised\)\);/);
  });

  it("gives sidebar search a visible keyboard focus state", () => {
    expect(appCss).toMatch(/\.sidebar-actions\s*{[^}]*gap:\s*6px;[^}]*margin-bottom:\s*8px;/);
    expect(appCss).toMatch(/\.sidebar-actions > button\s*{[^}]*width:\s*100%;[^}]*border:\s*0;[^}]*background:\s*transparent;[^}]*font-weight:\s*560;/);
    expect(appCss).toMatch(/\.sidebar-search\s*{[^}]*border:\s*1px solid transparent;[^}]*background:\s*color-mix\(in oklch, var\(--oa-raised\) 18%, transparent\);/);
    expect(appCss).toMatch(/\.sidebar-search:focus-within\s*{[^}]*border-color:\s*color-mix\(in oklch, var\(--oa-focus\) 64%, var\(--oa-border\)\);[^}]*outline:\s*0;/);
  });

  it("renders task startup as a distinct non-button composer action", () => {
    expect(appCss).toMatch(/\.composer-submit-pending\s*{[^}]*display:\s*inline-flex;[^}]*align-items:\s*center;[^}]*justify-content:\s*center;/);
    expect(appCss).toMatch(/\.composer-submit-pending svg\s*{[^}]*animation:\s*oa-spin 0\.9s linear infinite;/);
    expect(appCss).toMatch(/\.working-status-dots span,\s*\.composer-submit-pending svg\s*{[^}]*animation:\s*none;/);
  });

  it("renders external sessions inside the same sidebar list as tasks", () => {
    expect(appCss).toMatch(/\.sidebar\s*{[^}]*min-height:\s*0;[^}]*height:\s*100vh;[^}]*grid-template-rows:\s*auto auto minmax\(0, 1fr\) auto;[^}]*overflow:\s*hidden;/);
    expect(appCss).toMatch(/\.task-list\s*{[^}]*min-height:\s*0;[^}]*overflow:\s*auto;[^}]*scrollbar-gutter:\s*stable;[^}]*padding-right:\s*10px;[^}]*padding-bottom:\s*24px;/);
    expect(appCss).toMatch(/\.task-list\s*{[^}]*scroll-padding-bottom:\s*24px;/);
    expect(appCss).toMatch(/body\[data-shell="vscodeExtension"\] \.task-list\s*{[^}]*padding-right:\s*0;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.task-list\s*{[^}]*margin-right:\s*-8px;[^}]*padding-right:\s*4px;/);
    expect(appCss).toMatch(/\.sidebar-footer\s*{[^}]*border-top:\s*1px solid var\(--oa-border-subtle\);[^}]*padding-top:\s*6px;/);
    expect(appCss).toMatch(/\.settings-button\s*{\s*width:\s*100%;/);
    expect(appCss).toMatch(/\.task-trailing-meta\s*{[^}]*display:\s*inline-flex;[^}]*gap:\s*4px;[^}]*font-size:\s*11px;/);
    expect(appCss).not.toMatch(/\.task-trailing-agent-name\s*{/);
    expect(appCss).toMatch(/\.task-meta-age\s*{[^}]*color:\s*color-mix\(in oklch, var\(--oa-muted\) 82%, var\(--oa-text\)\);[^}]*flex:\s*0 0 auto;/);
    expect(appCss).not.toMatch(/\.task-meta-reference\s*{/);
    expect(appCss).toMatch(/\.agent-brand-icon\s*{[^}]*width:\s*11px;[^}]*height:\s*11px;/);
    expect(appCss).toMatch(/\.openai-agent-icon\s*{[^}]*color:\s*color-mix\(in oklch, var\(--oa-muted\) 76%, var\(--oa-text\)\);/);
    expect(appCss).not.toMatch(/\.state-mark\s*{/);
    expect(appCss).toMatch(/\.session-more\s*{[^}]*border-top:\s*1px solid color-mix\(in oklch, var\(--oa-border\) 62%, transparent\);[^}]*margin-top:\s*6px;/);
    expect(appCss).toMatch(/\.external-session-row \.task-open\s*{[^}]*grid-template-columns:\s*16px minmax\(0,\s*1fr\);/);
    expect(appCss).not.toMatch(/\.sidebar-sessions\s*{/);
    expect(appCss).not.toMatch(/\.native-session-panel\s*{/);
  });

  it("keeps the web workbench usable on mobile with an off-canvas task drawer", () => {
    expect(appCss).toMatch(/@media \(max-width:\s*760px\)\s*{[^}]*body\[data-shell="web"\] \.web-workbench-shell\s*{[^}]*grid-template-rows:\s*auto minmax\(0,\s*1fr\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.mobile-workbench-bar\s*{[^}]*z-index:\s*40;[^}]*display:\s*grid;[^}]*grid-template-columns:\s*44px minmax\(0,\s*1fr\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.mobile-workbench-bar button\s*{[^}]*width:\s*44px;[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar\s*{[^}]*position:\s*fixed;[^}]*pointer-events:\s*none;[^}]*transform:\s*translateX\(calc\(\(var\(--mobile-navigation-progress\) - 1\) \* 100%\)\);[^}]*visibility:\s*hidden;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\.mobile-navigation-open \.sidebar\s*{[^}]*pointer-events:\s*auto;[^}]*visibility:\s*visible;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.mobile-navigation-backdrop\s*{[^}]*display:\s*block;[^}]*position:\s*fixed;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-main-surface\s*{[^}]*grid-column:\s*1;[^}]*grid-row:\s*2;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-main-surface \.task-surface,[^{]+{\s*height:\s*100%;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-main-surface \.task-surface:not\(\.task-loading\)\s*{\s*grid-template-rows:\s*minmax\(0,\s*1fr\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.chat-column\s*{[^}]*height:\s*100%;[^}]*grid-template-rows:\s*minmax\(0,\s*1fr\) auto;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.sidebar-search input\s*{[^}]*min-height:\s*44px;[^}]*font-size:\s*16px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.project-task-group-header\s*{[^}]*min-height:\s*44px;[^}]*height:\s*auto;[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.project-task-group-toggle\s*{[^}]*min-height:\s*44px;[^}]*height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.composer-popover button\s*{[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.settings-header button\s*{[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.settings-header \.settings-title-button\s*{[^}]*min-height:\s*44px;[^}]*display:\s*inline-flex;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.settings-tabs button\s*{[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.settings-panel-title button\s*{[^}]*min-height:\s*44px;/);
    expect(appCss).toMatch(/\.composer-pill,[^{]+\.composer-submit-pending\s*{[^}]*min-height:\s*30px;/);
    expect(appCss).toMatch(/\.composer-controls \.composer-icon-button,[^{]+\.composer-submit-pending,[^{]+\.composer-footer > \.composer-icon-button\s*{[^}]*min-width:\s*36px;[^}]*width:\s*36px;/);
    expect(appCss).toMatch(/\.composer-actions \.composer-icon-button,[^{]+\.composer-submit-pending,[^{]+\.composer-footer > \.composer-icon-button\s*{[^}]*min-height:\s*36px;[^}]*height:\s*36px;/);
    expect(appCss).toMatch(/\.new-task-context-controls \.composer-pill\s*{\s*min-height:\s*30px;/);
    expect(appCss).toMatch(/\.composer-footer\s*{[^}]*display:\s*grid;[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto;[^}]*align-items:\s*center;[^}]*gap:\s*8px;/);
    expect(appCss).toMatch(/\.composer-footer > \.composer-icon-button,[^{]+\.composer-footer > \.composer-submit-pending\s*{[^}]*grid-column:\s*2;[^}]*justify-self:\s*end;/);
    expect(appCss).toMatch(/\.composer-controls\s*{[^}]*display:\s*flex;[^}]*flex-wrap:\s*nowrap;[^}]*overflow-x:\s*visible;/);
    expect(appCss).toMatch(/\.composer-adaptive-options\s*{[^}]*min-width:\s*0;[^}]*display:\s*flex;[^}]*flex:\s*1 1 auto;[^}]*gap:\s*4px;/);
    expect(appCss).toMatch(/\.composer-controls\s*{[^}]*position:\s*relative;/);
    expect(appCss).toMatch(/\.task-surface:not\(\.new-task-surface\) \.composer-controls\s*{[^}]*position:\s*static;/);
    expect(appCss).toMatch(/\.composer-adaptive-options\s*{[^}]*position:\s*static;/);
    expect(appCss).toMatch(/\.composer-overflow-options-anchor\s*{[^}]*position:\s*static;/);
    expect(appCss).toMatch(/\.composer-options-measurement\s*{[^}]*position:\s*fixed;[^}]*visibility:\s*hidden;[^}]*pointer-events:\s*none;/);
    expect(appCss).toMatch(/\.composer-overflow-options-anchor > \.composer-popover\s*{[^}]*left:\s*auto;[^}]*right:\s*0;/);
    expect(appCss).not.toMatch(/\.composer-config-control,[^{]+\.composer-isolation-control\s*{\s*display:\s*none;/);
    expect(appCss).not.toMatch(/\.composer-mobile-options-anchor\s*{/);
    expect(appCss).toMatch(/\.composer-controls \.composer-pill\s*{[^}]*width:\s*fit-content;[^}]*max-width:\s*min\(260px,\s*100%\);[^}]*flex:\s*0 1 auto;/);
    expect(appCss).toMatch(/\.composer-pill:not\(:disabled\),[^{]+\.composer-popover button:not\(:disabled\)\s*{[^}]*cursor:\s*pointer;/);
    expect(appCss).toMatch(/\.composer-overflow-menu-row:hover:not\(:disabled\) small,[^{]+{[^}]*color:\s*var\(--oa-text\);/);
    expect(appCss).toMatch(/@media \(hover:\s*none\)\s*{[^}]*\.composer \.composer-pill:hover:not\(:disabled\)\s*{[^}]*background:\s*transparent;/);
    expect(appCss).not.toMatch(/\.composer-menu-anchor > \.composer-popover:not\(\.composer-file-browser-popover\)\s*{[^}]*position:\s*static;/);
    expect(appCss).not.toMatch(/\.composer-option-anchor > \.composer-popover:not\(\.composer-file-browser-popover\)\s*{[^}]*position:\s*static;/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.composer-pill\s*{\s*max-width:\s*128px;/);
  });

  it("keeps mobile drawer exit motion visible and tracks direct swipe progress", () => {
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\s*{[^}]*--mobile-navigation-progress:\s*0;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\s*{[^}]*touch-action:\s*pan-y pinch-zoom;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar\s*{[^}]*transform:\s*translateX\(calc\(\(var\(--mobile-navigation-progress\) - 1\) \* 100%\)\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar\s*{[^}]*transition:[^;}]*transform 220ms cubic-bezier\(0\.22, 1, 0\.36, 1\)[^;}]*visibility 0s linear 220ms;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\.mobile-navigation-open\s*{[^}]*--mobile-navigation-progress:\s*1;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\.mobile-navigation-open \.sidebar\s*{[^}]*visibility:\s*visible;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell\.mobile-navigation-dragging \.sidebar\s*{[^}]*visibility:\s*visible;[^}]*transition:\s*none;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.mobile-navigation-backdrop\s*{[^}]*opacity:\s*var\(--mobile-navigation-progress, 0\);[^}]*transition:\s*opacity 180ms cubic-bezier\(0\.22, 1, 0\.36, 1\);/);
    expect(appCss).toMatch(/@media \(prefers-reduced-motion:\s*reduce\)\s*{[^}]*body\[data-shell="web"\] \.web-workbench-shell\.mobile-navigation-open \.sidebar,[^{]+body\[data-shell="web"\] \.web-workbench-shell \.sidebar,[^{]+body\[data-shell="web"\] \.mobile-navigation-backdrop\s*{\s*transition:\s*none;/);
  });

  it("shows focus rings only for keyboard-driven button focus", () => {
    expect(appCss).toMatch(/body\[data-input-modality="pointer"\] button:focus-visible\s*{\s*outline:\s*0;/);
  });

  it("does not leave sidebar hover styling latched on touch devices", () => {
    expect(appCss).toMatch(/@media \(hover:\s*none\)\s*{[^}]*body\[data-shell="web"\] \.sidebar \.settings-button:not\(\.selected\):hover,[^{]+body\[data-shell="web"\] \.sidebar \.sidebar-actions button:hover,[^{]+body\[data-shell="web"\] \.sidebar \.task-row:not\(\.selected\):hover\s*{[^}]*background:\s*transparent;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.sidebar \.settings-button:not\(\.selected\):active,[^{]+body\[data-shell="web"\] \.sidebar \.sidebar-actions button:active,[^{]+body\[data-shell="web"\] \.sidebar \.task-row:not\(\.selected\):active\s*{[^}]*background:\s*var\(--oa-raised\);/);
  });

  it("keeps mobile sidebar rows visually clear of the footer action", () => {
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar\s*{[^}]*grid-template-rows:\s*auto auto minmax\(0,\s*1fr\) auto;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar\s*{[^}]*width:\s*min\(288px,\s*calc\(100vw - 96px\)\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.sidebar-actions > button\s*{[^}]*width:\s*100%;[^}]*white-space:\s*nowrap;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar \.task-list\s*{[^}]*overflow-x:\s*hidden;[^}]*overflow-y:\s*auto;[^}]*scrollbar-gutter:\s*stable;[^}]*margin-right:\s*-8px;[^}]*padding-right:\s*4px;[^}]*padding-bottom:\s*24px;[^}]*scroll-padding-bottom:\s*24px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.sidebar \.sidebar-footer\s*{[^}]*position:\s*relative;[^}]*z-index:\s*1;[^}]*background:\s*var\(--oa-panel\);[^}]*box-shadow:\s*0 -12px 18px var\(--oa-panel\);/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.project-task-more,[^{]+\.web-workbench-shell \.session-more\s*{[^}]*position:\s*static;[^}]*z-index:\s*auto;/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.project-task-more\s*{\s*bottom:/);
    expect(appCss).not.toMatch(/body\[data-shell="web"\] \.web-workbench-shell \.session-more\s*{\s*bottom:/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.task-row\s*{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) 44px;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.task-row-action-slot\s*{[^}]*position:\s*relative;[^}]*width:\s*44px;[^}]*min-width:\s*44px;[^}]*pointer-events:\s*auto;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.task-row-action\s*{[^}]*min-width:\s*44px;[^}]*width:\s*44px;[^}]*opacity:\s*1;/);
    expect(appCss).toMatch(/body\[data-shell="web"\] \.task-row:has\(\.task-row-action:not\(:disabled\)\):hover \.task-trailing-meta,[^{]+body\[data-shell="web"\] \.task-row:has\(\.task-row-action:not\(:disabled\)\):has\(:focus-visible\) \.task-trailing-meta\s*{[^}]*visibility:\s*visible;/);
  });

  it("uses count-aware image layouts with overlaid composer removal controls", () => {
    expect(appCss).toMatch(/\.composer-attachment-list\[data-layout="single"\] \.composer-attachment-tile\s*{[^}]*width:\s*min\(144px,\s*100%\);[^}]*height:\s*104px;/);
    expect(appCss).toMatch(/\.composer-attachment-list\[data-layout="pair"\] \.composer-attachment-tile\s*{[^}]*width:\s*104px;[^}]*height:\s*80px;/);
    expect(appCss).toMatch(/\.composer-attachment-list\[data-layout="many"\] \.composer-attachment-tile\s*{[^}]*width:\s*76px;[^}]*height:\s*64px;/);
    expect(appCss).toMatch(/\.composer-image-remove\s*{[^}]*position:\s*absolute;[^}]*top:\s*4px;[^}]*right:\s*4px;/);
    expect(appCss).toMatch(/\.chat-attachment-list\[data-layout="single"\] \.chat-image-attachment\s*{[^}]*width:\s*min\(320px,\s*100%\);/);
    expect(appCss).toMatch(/\.chat-attachment-list\[data-layout="single"\] \.chat-attachment-chip\s*{[^}]*width:\s*min\(220px,\s*100%\);[^}]*height:\s*116px;[^}]*flex-direction:\s*column;/);
    expect(appCss).toMatch(/\.chat-attachment-list\[data-layout="pair"\] \.chat-attachment-chip\s*{[^}]*width:\s*min\(156px,\s*calc\(50% - 3px\)\);[^}]*height:\s*116px;/);
    expect(appCss).toMatch(/\.chat-attachment-list\[data-layout="many"\] \.chat-attachment-chip\s*{[^}]*width:\s*128px;[^}]*height:\s*96px;/);
    expect(appCss).toMatch(/\.chat-attachment-label\s*{[^}]*max-width:\s*100%;[^}]*overflow:\s*hidden;[^}]*text-overflow:\s*ellipsis;/);
    expect(appCss).toMatch(/\.chat-attachment-action-overlay\s*{[^}]*position:\s*absolute;[^}]*inset:\s*0;[^}]*opacity:\s*0;/);
    expect(appCss).toMatch(/\.chat-attachment-interactive:is\(:hover,\s*:focus-visible\) \.chat-attachment-action-overlay\s*{[^}]*opacity:\s*1;/);
    expect(appCss).toMatch(/\.chat-attachment-interactive::after\s*{[^}]*content:\s*attr\(data-attachment-tooltip\);[^}]*position:\s*absolute;/);
    expect(appCss).not.toContain(".chat-attachment-action-cue");
    expect(appCss).toMatch(/\.chat-image-preview\s*{[^}]*max-height:\s*240px;[^}]*object-fit:\s*contain;/);
  });

  it("shows attachment images at intrinsic size in a dismissible lightbox", () => {
    expect(appCss).toMatch(/\.attachment-preview-backdrop\s*{[^}]*position:\s*fixed;[^}]*inset:\s*0;[^}]*background:/);
    expect(appCss).toMatch(/\.attachment-preview-stage img\s*{[^}]*width:\s*auto;[^}]*height:\s*auto;[^}]*max-width:\s*calc\(100vw - 48px\);[^}]*max-height:\s*calc\(100vh - 48px\);/);
    expect(appCss).toMatch(/\.attachment-preview-close\s*{[^}]*position:\s*fixed;[^}]*width:\s*40px;[^}]*height:\s*40px;/);
  });

  it("opens the new-task project picker toward available viewport space", () => {
    expect(appCss).toMatch(/\.new-task-surface \.composer-file-browser-popover\s*{[^}]*top:\s*auto;[^}]*bottom:\s*calc\(100% \+ 6px\);/);
  });

  it("keeps the mobile project picker viewport-anchored with uncrowded file actions", () => {
    expect(appCss).toMatch(/@media \(max-width:\s*430px\)\s*{[^}]*\.new-task-surface \.composer-file-browser-popover\s*{[^}]*width:\s*calc\(100vw - 24px\);/);
    expect(appCss).toMatch(/\.new-task-surface \.composer-file-browser-popover\s*{[^}]*position:\s*fixed;[^}]*top:\s*auto;[^}]*left:\s*12px;[^}]*right:\s*auto;[^}]*bottom:\s*max\(12px,\s*env\(safe-area-inset-bottom\)\);[^}]*transform:\s*none;[^}]*max-height:\s*min\(420px,\s*calc\(100dvh - 96px\)\);/);
    expect(appCss).toMatch(/\.composer-file-row\.file\s*{[^}]*grid-template-columns:\s*18px minmax\(0,\s*1fr\);[^}]*grid-template-areas:\s*"icon label"\s*"actions actions";/);
    expect(appCss).toMatch(/\.composer-file-row-actions\s*{[^}]*grid-area:\s*actions;[^}]*grid-template-columns:\s*repeat\(2,\s*minmax\(0,\s*1fr\)\);/);
    expect(appCss).toMatch(/\.composer-file-row-actions button\s*{[^}]*min-height:\s*44px;/);
  });
});
