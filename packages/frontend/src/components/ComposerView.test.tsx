import { act, create, type ReactTestInstance } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentCommandsCatalog, ComposerSubmitShortcut, ConfigOptionCurrentValue, ConfigOptionsCatalog } from "@openaide/app-shell-contracts";
import type { FileBrowserEntryId, FileBrowserRootId } from "@openaide/app-server-client";
import type { AgentOption, ComposerAttachment, ComposerSelection } from "../state/composerOptions";
import { Composer } from "./Composer";
import { composerAvailability } from "./composerAvailability";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";

describe("Composer view behavior", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => vi.unstubAllGlobals());

  it("shows the composer focus boundary for keyboard navigation, not pointer focus", () => {
    const renderer = renderComposer();
    let composer = renderer.root.findByType("section");

    expect(composer.props["data-keyboard-focus"]).toBeUndefined();

    act(() => composer.props.onKeyDownCapture({ key: "Tab" }));
    composer = renderer.root.findByType("section");
    expect(composer.props["data-keyboard-focus"]).toBe("true");

    act(() => composer.props.onPointerDownCapture());
    composer = renderer.root.findByType("section");
    expect(composer.props["data-keyboard-focus"]).toBeUndefined();
  });

  it("renders removable attachments and closes open menus on Escape", () => {
    const onRemoveAttachment = vi.fn();
    const onRevealAttachment = vi.fn();
    const renderer = renderComposer({
      attachments: [attachment("attachment_1", "notes.md", "attachment-handle-1")],
      onRemoveAttachment,
      onRevealAttachment,
    });

    expect(text(renderer.root)).toContain("notes.md");
    click(buttonByLabel(renderer.root, "Add context"));
    expect(menuByLabel(renderer.root, "Add context")).toBeTruthy();

    act(() => {
      renderer.root.findByType("section").props.onKeyDown({ key: "Escape" });
    });
    expect(menusByLabel(renderer.root, "Add context")).toHaveLength(0);

    click(buttonByLabel(renderer.root, "Reveal notes.md"));
    expect(onRevealAttachment).toHaveBeenCalledWith("attachment_1");

    click(buttonByLabel(renderer.root, "Remove notes.md"));
    expect(onRemoveAttachment).toHaveBeenCalledWith("attachment_1");
  });

  it("closes the attachment menu when the user clicks elsewhere in the composer", () => {
    const listeners = new Map<string, (event: Event) => void>();
    vi.stubGlobal("document", {
      addEventListener: vi.fn((type: string, listener: (event: Event) => void) => listeners.set(type, listener)),
      removeEventListener: vi.fn((type: string) => listeners.delete(type)),
    });
    let renderer: ReturnType<typeof create> | undefined;
    act(() => {
      renderer = create(composerElement(), {
        createNodeMock: (element) => element.type === "section"
          ? { contains: () => true }
          : null,
      });
    });
    if (!renderer) throw new Error("Composer renderer was not created");

    click(buttonByLabel(renderer.root, "Add context"));
    expect(menuByLabel(renderer.root, "Add context")).toBeTruthy();

    act(() => listeners.get("pointerdown")?.({
      target: { closest: () => null },
    } as unknown as Event));

    expect(menusByLabel(renderer.root, "Add context")).toHaveLength(0);
  });

  it("applies a configuration choice after pointer down inside its menu", () => {
    const listeners = new Map<string, (event: Event) => void>();
    vi.stubGlobal("document", {
      addEventListener: vi.fn((type: string, listener: (event: Event) => void) => listeners.set(type, listener)),
      removeEventListener: vi.fn((type: string) => listeners.delete(type)),
    });
    const onSelectConfigOption = vi.fn();
    const renderer = renderComposer({ configOptions: configOptions(), onSelectConfigOption });

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    act(() => listeners.get("pointerdown")?.({
      target: {
        closest: (selector: string) => selector.includes(".composer-option-anchor") ? {} : null,
      },
    } as unknown as Event));

    expect(menusByLabel(renderer.root, "Reasoning")).toHaveLength(1);
    click(menuButtonByStrongLabel(renderer.root, "High"));
    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", { type: "id", value: "high" });
  });

  it("shows feedback after revealing an attachment", async () => {
    const onRevealAttachment = vi.fn(async () => undefined);
    const renderer = renderComposer({
      attachments: [attachment("attachment_1", "notes.md", "attachment-handle-1")],
      onRevealAttachment,
    });

    await clickAsync(buttonByLabel(renderer.root, "Reveal notes.md"));

    expect(onRevealAttachment).toHaveBeenCalledWith("attachment_1");
    expect(text(renderer.root)).toContain("Reveal requested.");
  });

  it("shows a recoverable error when attachment reveal fails", async () => {
    const onRevealAttachment = vi.fn(async () => {
      throw new Error("Shell unavailable");
    });
    const renderer = renderComposer({
      attachments: [attachment("attachment_1", "notes.md", "attachment-handle-1")],
      onRevealAttachment,
    });

    await clickAsync(buttonByLabel(renderer.root, "Reveal notes.md"));

    expect(text(renderer.root)).toContain("Unable to reveal notes.md.");
  });

  it("renders a single image as prominent media before the editor", () => {
    const renderer = renderComposer({
      attachments: [
        attachment(
          "attachment_1",
          "Clipboard image",
          "attachment-handle-1",
          "data:image/png;base64,aW1hZ2U=",
        ),
      ],
    });

    const preview = renderer.root.findByProps({ className: "composer-image-preview" });
    const openButton = buttonByLabel(renderer.root, "Open Clipboard image");
    const attachmentList = renderer.root.findByProps({ className: "composer-attachment-list", "data-layout": "single" });
    const composerHtml = JSON.stringify(renderer.toJSON());

    expect(preview.props.src).toBe("data:image/png;base64,aW1hZ2U=");
    expect(preview.props.alt).toBe("Clipboard image preview");
    expect(attachmentList).toBeTruthy();
    expect(composerHtml.indexOf("composer-attachment-list")).toBeLessThan(composerHtml.indexOf("composer-editor"));
    expect(text(openButton)).not.toContain("Clipboard image");
    expect(buttonsByLabel(renderer.root, "Reveal Clipboard image")).toHaveLength(0);
  });

  it("uses a denser image grid when several images are attached", () => {
    const renderer = renderComposer({
      attachments: [
        attachment("attachment_1", "one.png", "handle-1", "data:image/png;base64,b25l"),
        attachment("attachment_2", "two.png", "handle-2", "data:image/png;base64,dHdv"),
        attachment("attachment_3", "three.png", "handle-3", "data:image/png;base64,dGhyZWU="),
      ],
    });

    expect(renderer.root.findByProps({ className: "composer-attachment-list", "data-layout": "many" })).toBeTruthy();
    expect(renderer.root.findAllByProps({ className: "composer-image-preview" })).toHaveLength(3);
    expect(renderer.root.findAllByProps({ className: "composer-image-remove" })).toHaveLength(3);
  });

  it("keeps Images and files in one ordered attachment list", () => {
    const renderer = renderComposer({
      attachments: [
        attachment("image-1", "diagram.png", "image-handle", "data:image/png;base64,aW1hZ2U="),
        attachment("file-1", "notes.md", "file-handle"),
      ],
    });

    const lists = renderer.root.findAllByProps({ className: "composer-attachment-list" });
    expect(lists).toHaveLength(1);
    expect(lists[0].findAll((node) => node.props.className?.includes("composer-attachment-tile"))).toHaveLength(2);
    expect(lists[0].findByProps({ "data-file-kind": "markdown" })).toBeTruthy();
    expect(lists[0].findByProps({ title: "notes.md" })).toBeTruthy();
    const html = JSON.stringify(renderer.toJSON());
    expect(html.indexOf("diagram.png preview")).toBeLessThan(html.indexOf("notes.md"));
  });

  it("opens image attachment previews from composer tokens", () => {
    const renderer = renderComposer({
      attachments: [
        attachment(
          "attachment_1",
          "Clipboard image",
          "attachment-handle-1",
          "data:image/png;base64,aW1hZ2U=",
        ),
      ],
      canEdit: false,
    });

    click(buttonByLabel(renderer.root, "Open Clipboard image"));

    const lightbox = renderer.root.findByProps({ className: "attachment-preview-lightbox" });
    expect(text(lightbox)).not.toContain("Clipboard image");
    expect(renderer.root.findByProps({ className: "attachment-preview-stage" }).findByType("img").props.src).toBe(
      "data:image/png;base64,aW1hZ2U=",
    );
  });

  it("keeps device file actions separate from workspace mentions", () => {
    const renderer = renderComposer({ selection: selection({ workspaceRoot: "", workspaceLabel: "Workspace" }) });

    click(buttonByLabel(renderer.root, "Add context"));

    const menu = menuByLabel(renderer.root, "Add context");
    expect(menu.findAllByProps({ role: "menuitem" })).toHaveLength(2);
    expect(renderer.root.findAllByType("strong").some((node) => node.children.join("") === "Workspace files")).toBe(false);
    expect(text(menuButtonByStrongLabel(renderer.root, "Attach images"))).toContain("Choose images from this device.");
    expect(menuButtonByStrongLabel(renderer.root, "Attach files").props.disabled).toBe(true);
    expect(menuButtonByStrongLabel(renderer.root, "Attach images").props.disabled).toBe(true);
    expect(renderer.root.findAllByProps({ type: "file" })[0].props.disabled).toBe(true);
  });

  it("closes attachment popovers and locks add controls when Send starts", async () => {
    const fileBrowser = fileBrowserCallbacks();
    const renderer = renderComposer({ fileBrowser });

    click(buttonByLabel(renderer.root, "Add context"));
    expect(menuByLabel(renderer.root, "Add context")).toBeTruthy();

    act(() => {
      renderer.update(composerElement({
        canEdit: false,
        fileBrowser,
        prompt: "Sending",
        submissionAllowed: false,
        submitting: true,
      }));
    });

    expect(menusByLabel(renderer.root, "Add context")).toHaveLength(0);
    expect(buttonByLabel(renderer.root, "Add context").props.disabled).toBe(true);
  });

  it("blocks image selection and explains why a populated draft cannot send", () => {
    const renderer = renderComposer({
      imageAttachmentsAllowed: false,
      prompt: "what about now?",
      submissionAllowed: false,
      submissionBlockedMessage: "This Agent does not accept images.",
    });

    expect(buttonByLabel(renderer.root, "Add context").props.disabled).toBe(true);
    const blocker = renderer.root.find((node) => node.props.className?.includes("composer-submission-blocker"));
    expect(text(blocker)).toBe(
      "This Agent does not accept images.",
    );
  });

  it("uploads every selected image through the App Server attachment callback", async () => {
    const first = new File(["first"], "first.png", { type: "image/png" });
    const second = new File(["second"], "second.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    const renderer = renderComposer({ fileBrowser });

    click(buttonByLabel(renderer.root, "Add context"));

    const uploadButton = menuButtonByStrongLabel(renderer.root, "Attach images");
    expect(uploadButton.props.disabled).toBeFalsy();
    const input = renderer.root.findByProps({ type: "file", accept: "image/*" });
    expect(input.props.multiple).toBe(true);
    act(() => {
      input.props.onChange({
        currentTarget: { value: "upload.png" },
        target: { files: [first, second] },
      });
    });
    await settleRenderer();

    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(1, first);
    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(2, second);
    expect(menusByLabel(renderer.root, "Add context")).toHaveLength(0);
  });

  it("keeps file upload progress in the same attachment list, blocks Send, and cancels it", async () => {
    const selected = new File(["0123456789"], "model.bin");
    let uploadSignal: AbortSignal | undefined;
    const fileBrowser: TaskFileBrowserCallbacks = {
      ...fileBrowserCallbacks(),
      attachmentMode: "webUpload",
      attachFiles: vi.fn(async (_files, options) => {
        uploadSignal = options.signal;
        options.onProgress({ loaded: 5, total: 10 });
        await new Promise(() => undefined);
      }),
    };
    const renderer = renderComposer({
      attachments: [attachment("image-1", "diagram.png", "image-handle", "data:image/png;base64,aW1hZ2U=")],
      fileBrowser,
      prompt: "Use this",
    });

    click(buttonByLabel(renderer.root, "Add context"));
    const input = renderer.root.findAllByProps({ type: "file" }).find((node) => !node.props.accept);
    expect(input).toBeTruthy();
    act(() => input?.props.onChange({ currentTarget: { value: "model.bin" }, target: { files: [selected] } }));
    await settleRenderer();

    expect(fileBrowser.attachFiles).toHaveBeenCalledWith([selected], expect.objectContaining({ maxFiles: 1 }));
    const uploadTile = renderer.root.findByProps({
      className: "composer-attachment-tile composer-file-attachment composer-file-upload",
    });
    expect(text(uploadTile)).toContain("model.bin");
    expect(uploadTile.findByProps({ "aria-label": "Uploading model.bin" }).props.value).toBe(5);
    const attachmentList = renderer.root.findByProps({ className: "composer-attachment-list" });
    expect(attachmentList.props["data-layout"]).toBe("pair");
    expect(attachmentList.findAll((node) => node.props.className?.includes("composer-attachment-tile"))).toHaveLength(2);
    const composerHtml = JSON.stringify(renderer.toJSON());
    expect(composerHtml.indexOf("composer-file-upload")).toBeLessThan(composerHtml.indexOf("composer-editor"));
    expect(buttonByLabel(renderer.root, "Send message").props.disabled).toBe(true);
    click(buttonByLabel(renderer.root, "Cancel model.bin"));
    expect(uploadSignal?.aborted).toBe(true);
  });

  it("shows a failed file upload inline without using the tile tooltip for the error", async () => {
    const selected = new File(["support"], "openaide-support-report.zip");
    const fileBrowser: TaskFileBrowserCallbacks = {
      ...fileBrowserCallbacks(),
      attachmentMode: "webUpload",
      attachFiles: vi.fn().mockRejectedValue(new Error("File upload failed.")),
    };
    const renderer = renderComposer({ fileBrowser, prompt: "Inspect this" });

    click(buttonByLabel(renderer.root, "Add context"));
    const input = renderer.root.findAllByProps({ type: "file" }).find((node) => !node.props.accept);
    act(() => input?.props.onChange({
      currentTarget: { value: selected.name },
      target: { files: [selected] },
    }));
    await settleRenderer();

    const uploadTile = renderer.root.findByProps({
      className: "composer-attachment-tile composer-file-attachment composer-file-upload",
    });
    expect(uploadTile.props.title).toBeUndefined();
    expect(uploadTile.findByProps({ className: "composer-file-attachment-label" }).props.title).toBe(selected.name);
    const error = uploadTile.findByProps({ className: "composer-file-upload-error", role: "status" });
    expect(text(error)).toContain("Upload failed");
    expect(error.props["aria-live"]).toBe("polite");
  });

  it("retries a failed file upload without asking the user to select the file again", async () => {
    const selected = new File(["support"], "openaide-support-report.zip");
    const attachFiles = vi.fn()
      .mockRejectedValueOnce(new Error("File upload failed."))
      .mockResolvedValueOnce(undefined);
    const fileBrowser: TaskFileBrowserCallbacks = {
      ...fileBrowserCallbacks(),
      attachmentMode: "webUpload",
      attachFiles,
    };
    const renderer = renderComposer({ fileBrowser, prompt: "Inspect this" });

    click(buttonByLabel(renderer.root, "Add context"));
    const input = renderer.root.findAllByProps({ type: "file" }).find((node) => !node.props.accept);
    act(() => input?.props.onChange({
      currentTarget: { value: selected.name },
      target: { files: [selected] },
    }));
    await settleRenderer();

    click(buttonByLabel(renderer.root, `Retry ${selected.name}`));
    await settleRenderer();

    expect(attachFiles).toHaveBeenCalledTimes(2);
    expect(attachFiles).toHaveBeenLastCalledWith([selected], expect.objectContaining({ maxFiles: 1 }));
    expect(renderer.root.findAllByProps({
      className: "composer-attachment-tile composer-file-attachment composer-file-upload",
    })).toHaveLength(0);
  });

  it("keeps dropped Images native and routes other dropped files through file attachment", async () => {
    const image = new File(["image"], "diagram.png", { type: "image/png" });
    const file = new File(["data"], "model.bin");
    const fileBrowser: TaskFileBrowserCallbacks = {
      ...fileBrowserCallbacks(),
      attachmentMode: "webUpload",
      attachFiles: vi.fn(async () => undefined),
    };
    const renderer = renderComposer({ fileBrowser, prompt: "Inspect" });
    const preventDefault = vi.fn();

    act(() => textarea(renderer.root).props.onDrop({
      dataTransfer: { files: [image, file] },
      preventDefault,
    }));
    await settleRenderer();

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(fileBrowser.attachImage).toHaveBeenCalledWith(image, expect.objectContaining({ prompt: "Inspect" }));
    expect(fileBrowser.attachFiles).toHaveBeenCalledWith([file], expect.objectContaining({ maxFiles: 1 }));
  });

  it("attaches every pasted image through the App Server file browser callbacks", async () => {
    const first = new File(["first"], "first.png", { type: "image/png" });
    const second = new File(["second"], "second.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    const preventDefault = vi.fn();
    const onChange = vi.fn();
    const renderer = renderComposer({ fileBrowser, onChange });
    const input = textarea(renderer.root);

    inputText(input, "Explain this screenshot");

    paste(input, {
      clipboardData: {
        items: [
          { kind: "file", type: "image/png", getAsFile: () => first },
          { kind: "file", type: "image/png", getAsFile: () => second },
        ],
      },
      preventDefault,
    });
    await settleRenderer();

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("Explain this screenshot");
    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(1, first, {
      prompt: "Explain this screenshot",
      context: [],
    });
    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(2, second, {
      prompt: "Explain this screenshot",
      context: [],
    });
  });

  it("continues attaching later pasted images when one image fails", async () => {
    const first = new File(["first"], "first.png", { type: "image/png" });
    const second = new File(["second"], "second.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    fileBrowser.attachImage = vi.fn(async (file) => {
      if (file === first) throw new Error("First image is too large.");
    });
    const onUnsupportedImagePaste = vi.fn();
    const renderer = renderComposer({ fileBrowser, onUnsupportedImagePaste });

    paste(textarea(renderer.root), {
      clipboardData: {
        items: [
          { kind: "file", type: "image/png", getAsFile: () => first },
          { kind: "file", type: "image/png", getAsFile: () => second },
        ],
      },
      preventDefault: vi.fn(),
    });
    await settleRenderer();

    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(1, first, { prompt: "", context: [] });
    expect(fileBrowser.attachImage).toHaveBeenNthCalledWith(2, second, { prompt: "", context: [] });
    expect(onUnsupportedImagePaste).toHaveBeenCalledWith("First image is too large.");
  });

  it("attaches pasted image clipboard files when clipboard items are unavailable", () => {
    const file = new File(["image"], "pasted.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    const preventDefault = vi.fn();
    const renderer = renderComposer({ fileBrowser });

    paste(textarea(renderer.root), {
      clipboardData: {
        items: [],
        files: [file],
      },
      preventDefault,
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(fileBrowser.attachImage).toHaveBeenCalledWith(file, {
      prompt: "",
      context: [],
    });
  });

  it("reports pasted image clipboard data when no Task attachment backend is available", () => {
    const onUnsupportedImagePaste = vi.fn();
    const preventDefault = vi.fn();
    const renderer = renderComposer({ onUnsupportedImagePaste });

    paste(textarea(renderer.root), {
      clipboardData: {
        items: [{ kind: "file", type: "image/png", getAsFile: () => new File(["image"], "pasted.png", { type: "image/png" }) }],
      },
      preventDefault,
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(onUnsupportedImagePaste).toHaveBeenCalledWith("Images can be attached after the Task is open.");
  });

  it("filters disabled Agents and closes the Agent menu after selection", () => {
    const onSelectAgent = vi.fn();
    const agents: AgentOption[] = [
      { id: "codex", label: "Codex", description: "Codex agent.", icon: "openai" },
      { id: "disabled", label: "Disabled", description: "Disabled agent.", icon: "bot", enabled: false },
    ];
    const renderer = renderComposer({ agents, onSelectAgent });

    click(buttonByText(renderer.root, "Codex"));
    expect(text(menuByLabel(renderer.root, "Agent"))).toContain("Codex agent.");
    expect(text(menuByLabel(renderer.root, "Agent"))).not.toContain("Disabled");

    const codexButton = menuButtonByStrongLabel(renderer.root, "Codex");
    expect(codexButton.props.role).toBe("menuitemradio");
    expect(codexButton.props["aria-checked"]).toBe(true);
    expect(codexButton.findByProps({ className: "composer-menu-selection" }).findAllByType("svg")).toHaveLength(1);

    click(codexButton);
    expect(onSelectAgent).toHaveBeenCalledWith("codex");
    expect(menusByLabel(renderer.root, "Agent")).toHaveLength(0);
  });

  it("renders config and isolation menus with selected values and callbacks", () => {
    const onSelectConfigOption = vi.fn();
    const onSelectIsolation = vi.fn();
    const renderer = renderComposer({
      configOptions: configOptions(),
      onSelectConfigOption,
      onSelectIsolation,
      selection: selection({ isolation: "git_worktree" }),
    });

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    const reasoningMenu = menuByLabel(renderer.root, "Reasoning");
    expect(text(reasoningMenu)).toContain("Reasoning");
    expect(text(reasoningMenu)).toContain("Reasoning effort.");
    expect(text(reasoningMenu)).toContain("Higher accuracy.");
    click(menuButtonByStrongLabel(renderer.root, "High"));
    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", { type: "id", value: "high" });
    const pendingControl = renderer.root.findByProps({
      "aria-label": "High, updating Agent option",
    });
    expect(text(pendingControl)).toBe("High");
    expect(pendingControl.props["aria-busy"]).toBe(true);

    click(buttonByText(renderer.root, "Worktree"));
    const worktreeButton = menuButtonByStrongLabel(renderer.root, "Worktree");
    expect(worktreeButton.props["aria-checked"]).toBe(true);
    click(menuButtonByStrongLabel(renderer.root, "Docker"));
    expect(onSelectIsolation).toHaveBeenCalledWith("docker");
  });

  it("groups a trailing option suffix and keeps grouped values selectable", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const onSelectConfigOption = vi.fn();
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "model",
            kind: "select", current_value: { type: "id", value: "gpt-5.6" },
            id: "model",
            label: "Model",
            values: [{ id: "gpt-5.6", label: "GPT-5.6" }],
          },
          ...configOptions().options,
        ],
        status: "ready",
      },
      onSelectConfigOption,
      selection: selection({ isolation: "git_worktree" }),
    });

    click(buttonByText(renderer.root, "More · 3"));
    expect(text(menuByLabel(renderer.root, "More options"))).toContain("Current: Balanced");

    click(menuButtonByStrongLabel(renderer.root, "Reasoning"));
    expect(text(menuByLabel(renderer.root, "Reasoning"))).toContain("Higher accuracy.");
    click(menuButtonByStrongLabel(renderer.root, "High"));

    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", { type: "id", value: "high" });
    expect(text(menuByLabel(renderer.root, "More options"))).toContain("Reasoning");
  });

  it("closes a grouped option submenu when More is pressed again", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "model",
            kind: "select", current_value: { type: "id", value: "gpt-5.6" },
            id: "model",
            label: "Model",
            values: [{ id: "gpt-5.6", label: "GPT-5.6" }],
          },
          ...configOptions().options,
        ],
        status: "ready",
      },
    });

    click(buttonByText(renderer.root, "More · 3"));
    click(menuButtonByStrongLabel(renderer.root, "Reasoning"));
    expect(menuByLabel(renderer.root, "Reasoning")).toBeTruthy();

    click(buttonByText(renderer.root, "More · 3"));

    expect(menusByLabel(renderer.root, "Reasoning")).toHaveLength(0);
    expect(menusByLabel(renderer.root, "More options")).toHaveLength(0);
  });

  it("keeps grouped Configuration Options locked while leaving mutable Isolation available", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const renderer = renderComposer({
      configLocked: true,
      configOptions: configOptions(),
      showIsolationSelector: true,
    });
    const compactAnchor = renderer.root.findByProps({ className: "composer-option-anchor composer-overflow-options-anchor" });

    click(compactAnchor.findByType("button"));

    expect(menuButtonByStrongLabel(renderer.root, "Reasoning").props.disabled).toBe(true);
    expect(menuButtonByStrongLabel(renderer.root, "Isolation").props.disabled).toBe(false);
  });

  it("disables More when every grouped control is locked", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const renderer = renderComposer({
      configLocked: true,
      configOptions: configOptions(),
      showIsolationSelector: false,
    });
    const compactAnchor = renderer.root.findByProps({ className: "composer-option-anchor composer-overflow-options-anchor" });

    expect(compactAnchor.findByType("span").props.className).toContain("locked");
  });

  it("shows the requested option as pending before explaining a slow Agent update", () => {
    vi.useFakeTimers();
    const renderer = renderComposer({
      configLocked: true,
      configOptions: {
        agent_id: "codex",
        pending_change: {
          mutation_id: "mutation-1",
          option_id: "fast-mode",
          requested_value: { type: "id", value: "on" },
        },
        options: [{
          kind: "select", current_value: { type: "id", value: "off" },
          id: "fast-mode",
          label: "Fast mode",
          values: [
            { id: "off", label: "Off" },
            { id: "on", label: "On" },
          ],
        }],
        status: "ready",
      },
      showIsolationSelector: false,
    });

    const pendingControl = renderer.root.findByProps({ "aria-label": "On, updating Agent option" });
    expect(text(pendingControl)).toBe("On");
    expect(renderer.root.findAllByProps({ "aria-busy": true })).toHaveLength(1);
    expect(text(renderer.root)).not.toContain("Agent is still updating options");

    act(() => {
      vi.advanceTimersByTime(5_000);
    });

    expect(text(renderer.root)).toContain("Agent is still updating options");
    vi.useRealTimers();
  });

  it("renders a boolean option as a text-first switch and sends its inverse typed value", () => {
    const onSelectConfigOption = vi.fn();
    const renderer = renderComposer({
      configOptions: booleanConfigOptions(),
      onSelectConfigOption,
      showIsolationSelector: false,
    });

    const control = buttonByLabel(renderer.root, "Brave mode: Off");
    expect(control.props.role).toBe("switch");
    expect(control.props["aria-checked"]).toBe(false);
    expect(control.props.title).toBeUndefined();
    expect(text(control)).toBe("Brave mode");
    expect(control.findAllByType("svg")).toHaveLength(0);

    click(control);

    expect(onSelectConfigOption).toHaveBeenCalledWith(
      "brave_mode",
      { type: "boolean", value: true },
    );
  });

  it("describes direct Configuration Options with rich hover and focus cards", () => {
    const renderer = renderComposer({
      configOptions: {
        ...configOptions(),
        options: [...configOptions().options, ...booleanConfigOptions().options],
      },
      showIsolationSelector: false,
    });

    const selectControl = configControlButtonsByText(renderer.root, "Balanced")[0];
    const selectAnchor = ancestorWithClass(selectControl, "composer-option-anchor");
    const selectTooltip = selectAnchor?.findByProps({ role: "tooltip" });
    expect(text(selectTooltip as ReactTestInstance)).toBe("ReasoningReasoning effort.");
    expect(selectControl.props["aria-describedby"]).toBe(selectTooltip?.props.id);

    const booleanControl = buttonByLabel(renderer.root, "Brave mode: Off");
    const booleanAnchor = ancestorWithClass(booleanControl, "composer-option-anchor");
    const booleanTooltip = booleanAnchor?.findByProps({ role: "tooltip" });
    expect(text(booleanTooltip as ReactTestInstance)).toBe("Brave modeSkip confirmation prompts.");
    expect(booleanControl.props["aria-describedby"]).toBe(booleanTooltip?.props.id);
  });

  it("requires fresh pointer movement before showing option hover details after a click", () => {
    const renderer = renderComposer({
      configOptions: configOptions(),
      showIsolationSelector: false,
    });
    const control = configControlButtonsByText(renderer.root, "Balanced")[0];
    const anchor = ancestorWithClass(control, "composer-config-control-anchor")!;

    act(() => anchor.props.onPointerMove());
    expect(ancestorWithClass(control, "composer-config-control-anchor")?.props["data-hover-armed"]).toBe(true);

    act(() => anchor.props.onPointerDown());
    click(control);
    click(control);

    expect(ancestorWithClass(control, "composer-config-control-anchor")?.props["data-hover-armed"]).toBeUndefined();
  });

  it("shows the next option hover immediately after one option hover has activated", () => {
    vi.useFakeTimers();
    try {
      const renderer = renderComposer({
        configOptions: {
          ...configOptions(),
          options: [...configOptions().options, ...booleanConfigOptions().options],
        },
        showIsolationSelector: false,
      });
      const select = configControlButtonsByText(renderer.root, "Balanced")[0];
      const boolean = buttonByLabel(renderer.root, "Brave mode: Off");
      const selectAnchor = ancestorWithClass(select, "composer-config-control-anchor")!;

      act(() => selectAnchor.props.onPointerMove());
      act(() => {
        vi.advanceTimersByTime(600);
      });

      const booleanAnchor = ancestorWithClass(boolean, "composer-config-control-anchor")!;
      expect(booleanAnchor.props["data-hover-quick"]).toBe(true);
      act(() => booleanAnchor.props.onPointerMove());
      expect(ancestorWithClass(boolean, "composer-config-control-anchor")?.props["data-hover-armed"]).toBe(true);

      const options = renderer.root.findByProps({ className: "composer-adaptive-options" });
      act(() => options.props.onPointerLeave());
      expect(ancestorWithClass(boolean, "composer-config-control-anchor")?.props["data-hover-quick"]).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("settles immediate pending presentation from the authoritative Agent catalog", () => {
    const catalog = configOptions();
    const renderer = renderComposer({ configOptions: catalog });

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    click(menuButtonByStrongLabel(renderer.root, "High"));
    expect(renderer.root.findAllByProps({ "aria-label": "High, updating Agent option" })).toHaveLength(1);

    act(() => {
      renderer.update(composerElement({
        configOptions: {
          ...catalog,
          options: catalog.options.map((option) => ({
            ...option,
            current_value: { type: "id" as const, value: "high" },
          })),
        },
      }));
    });

    expect(configControlButtonsByText(renderer.root, "High")).toHaveLength(1);
    expect(renderer.root.findAllByProps({ "aria-busy": true })).toHaveLength(0);
  });

  it("clears immediate pending presentation when the mutation fails", () => {
    const catalog = configOptions();
    const renderer = renderComposer({ configOptions: catalog });

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    click(menuButtonByStrongLabel(renderer.root, "High"));

    act(() => {
      renderer.update(composerElement({ configOptions: catalog, error: "Unable to update Agent option." }));
    });

    expect(configControlButtonsByText(renderer.root, "Balanced")).toHaveLength(1);
    expect(renderer.root.findAllByProps({ "aria-busy": true })).toHaveLength(0);
  });

  it("shows a pending boolean's requested state without changing its content width", () => {
    const catalog = booleanConfigOptions();
    const renderer = renderComposer({
      configLocked: true,
      configOptions: {
        ...catalog,
        pending_change: {
          mutation_id: "mutation-1",
          option_id: "brave_mode",
          requested_value: { type: "boolean", value: true },
        },
      },
      showIsolationSelector: false,
    });

    const control = buttonByLabel(renderer.root, "Brave mode: On, updating Agent option");
    expect(control.props["aria-checked"]).toBe(true);
    expect(control.props["aria-busy"]).toBe(true);
    expect(control.props.className).toContain("pending");
    expect(text(control)).toBe("Brave mode");
    expect(control.findAllByType("svg")).toHaveLength(0);
  });

  it("toggles a grouped boolean without closing the ordered overflow", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const onSelectConfigOption = vi.fn();
    const renderer = renderComposer({
      configOptions: booleanConfigOptions(),
      onSelectConfigOption,
      showIsolationSelector: false,
    });

    click(buttonByText(renderer.root, "More · 1"));
    const overflow = menuByLabel(renderer.root, "More options");
    const control = buttonByLabel(overflow, "Brave mode: Off");
    click(control);

    expect(onSelectConfigOption).toHaveBeenCalledWith(
      "brave_mode",
      { type: "boolean", value: true },
    );
    expect(menuByLabel(renderer.root, "More options")).toBeTruthy();
  });

  it("keeps grouped configuration rows iconless and the boolean indicator beside its label", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const renderer = renderComposer({
      configOptions: {
        ...configOptions(),
        options: [...configOptions().options, ...booleanConfigOptions().options],
      },
      showIsolationSelector: false,
    });

    click(buttonByText(renderer.root, "More · 2"));
    const overflow = menuByLabel(renderer.root, "More options");
    const reasoning = menuButtonByStrongLabel(overflow, "Reasoning");
    const boolean = buttonByLabel(overflow, "Brave mode: Off");
    const booleanCopy = boolean.findByProps({ className: "composer-boolean-copy" });

    expect(reasoning.findAll((node) =>
      node.type === "svg" && String(node.props.className).includes("lucide-brain"),
    )).toHaveLength(0);
    expect(booleanCopy.findAllByProps({ className: "composer-boolean-indicator" })).toHaveLength(1);

    click(reasoning);
    const high = menuButtonByStrongLabel(renderer.root, "High");
    expect(high.findAll((node) =>
      node.type === "svg" && String(node.props.className).includes("lucide-brain"),
    )).toHaveLength(0);
  });

  it("marks More as pending when the changing boolean is grouped", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const catalog = booleanConfigOptions();
    const renderer = renderComposer({
      configLocked: true,
      configOptions: {
        ...catalog,
        pending_change: {
          mutation_id: "mutation-1",
          option_id: "brave_mode",
          requested_value: { type: "boolean", value: true },
        },
      },
      showIsolationSelector: false,
    });

    const more = renderer.root.findByProps({
      "aria-label": "More · 1, updating Agent option",
    });
    expect(more.props["aria-busy"]).toBe(true);
    expect(more.props.className).toContain("pending");
    expect(text(more)).toBe("More · 1");
  });

  it("labels mode config controls with the compact selected value", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "mode",
            kind: "select", current_value: { type: "id", value: "agent" },
            id: "mode",
            label: "Mode",
            values: [
              { id: "read_only", label: "Read-only" },
              { id: "agent", label: "Agent" },
            ],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "Agent")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "Mode: Agent")).toHaveLength(0);
  });

  it("humanizes config value ids instead of falling back to generic option labels", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "mode",
            kind: "select", current_value: { type: "id", value: "agent" },
            id: "mode",
            label: "mode",
            values: [{ id: "read_only", label: "Read-only" }],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "Agent")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "mode")).toHaveLength(0);
  });

  it("preserves common model id capitalization when value labels are missing", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "model",
            kind: "select", current_value: { type: "id", value: "gpt-5.5" },
            id: "model",
            label: "model",
            values: [],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "GPT-5.5")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "Gpt 5.5")).toHaveLength(0);
  });

  it("shows current values without Configuration Option title prefixes", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "mode",
            kind: "select", current_value: { type: "id", value: "agent" },
            id: "mode",
            label: "Mode",
            values: [{ id: "agent", label: "Agent" }],
          },
          {
            kind: "select", current_value: { type: "id", value: "off" },
            id: "fast",
            label: "Fast mode",
            values: [{ id: "off", label: "Off" }],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "Agent")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "Off")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "Fast: Off")).toHaveLength(0);
  });

  it("positions a configuration menu from the control that opened it", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            kind: "select", current_value: { type: "id", value: "off" },
            id: "fast",
            label: "Fast mode",
            values: [{ id: "off", label: "Off" }],
          },
        ],
        status: "ready",
      },
    });

    click(configControlButtonsByText(renderer.root, "Off")[0]);

    const menuAnchor = ancestorWithClass(menuByLabel(renderer.root, "Fast mode"), "composer-option-anchor");
    expect(menuAnchor?.props.className).toContain("composer-option-anchor");
    expect(text(menuAnchor as ReactTestInstance)).toContain("Off");
  });

  it("normalizes lowercase config value labels in compact controls", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "thought_level",
            kind: "select", current_value: { type: "id", value: "medium" },
            id: "reasoning",
            label: "Reasoning",
            values: [{ id: "medium", label: "medium" }],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "Medium")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "medium")).toHaveLength(0);
  });

  it("keeps direct configuration controls text-first", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "thought_level",
            kind: "select", current_value: { type: "id", value: "medium" },
            id: "reasoning_effort",
            label: "Reasoning effort",
            values: [{ id: "medium", label: "Medium" }],
          },
          {
            category: "other",
            kind: "select", current_value: { type: "id", value: "on" },
            id: "fast-mode",
            label: "Fast mode",
            values: [{ id: "on", label: "On" }],
          },
        ],
        status: "ready",
      },
    });

    const reasoningIcons = configControlButtonsByText(renderer.root, "Medium")[0].findAllByType("svg");
    const otherIcons = configControlButtonsByText(renderer.root, "On")[0].findAllByType("svg");

    expect(reasoningIcons.filter((icon) => String(icon.props.className).includes("lucide-brain"))).toHaveLength(0);
    expect(otherIcons.filter((icon) => String(icon.props.className).includes("lucide-sliders-horizontal"))).toHaveLength(0);
    expect(reasoningIcons.filter((icon) => String(icon.props.className).includes("lucide-chevron-down"))).toHaveLength(1);
    expect(otherIcons.filter((icon) => String(icon.props.className).includes("lucide-chevron-down"))).toHaveLength(1);
  });

  it("can hide Agent and Isolation controls while keeping config option controls", () => {
    const onSelectConfigOption = vi.fn();
    const renderer = renderComposer({
      configOptions: configOptions(),
      onSelectConfigOption,
      showAgentSelector: false,
      showIsolationSelector: false,
    });

    expect(buttonsByText(renderer.root, "Codex")).toHaveLength(0);
    expect(buttonsByText(renderer.root, "Local")).toHaveLength(0);

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    expect(text(menuByLabel(renderer.root, "Reasoning"))).toContain("Higher accuracy.");
    click(menuButtonByStrongLabel(renderer.root, "High"));
    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", { type: "id", value: "high" });
  });

  it("closes an open configuration menu when configuration becomes locked", () => {
    const props = {
      configOptions: configOptions(),
      showAgentSelector: false,
      showIsolationSelector: false,
    };
    const renderer = renderComposer(props);

    click(configControlButtonsByText(renderer.root, "Balanced")[0]);
    expect(menusByLabel(renderer.root, "Reasoning")).toHaveLength(1);

    act(() => {
      renderer.update(composerElement({ ...props, configLocked: true }));
    });

    expect(menusByLabel(renderer.root, "Reasoning")).toHaveLength(0);
  });

  it("preserves locked controls while showing stop instead of send", () => {
    const onSubmit = vi.fn();
    const onCancel = vi.fn();
    const lockedRenderer = renderComposer({
      agentLocked: true,
      configLocked: true,
      configOptions: configOptions(),
      onCancel,
    });

    expect(lockedRenderer.root.findAll((node) =>
      typeof node.props.className === "string" &&
      node.props.className.split(/\s+/).includes("composer-pill") &&
      node.props.className.split(/\s+/).includes("locked"),
    )).toHaveLength(3);
    expect(buttonsByLabel(lockedRenderer.root, "Send message")).toHaveLength(0);
    expect(buttonsByLabel(lockedRenderer.root, "Stop task")).toHaveLength(1);
    click(buttonByLabel(lockedRenderer.root, "Stop task"));
    expect(onCancel).toHaveBeenCalledTimes(1);

    const sendRenderer = renderComposer({ onSubmit, submissionAllowed: false });
    const sendButton = buttonByLabel(sendRenderer.root, "Send message");
    expect(sendButton.props.disabled).toBe(true);
  });

  it("switches an active task from stop to send when the composer has a draft", () => {
    const onSubmit = vi.fn();
    const onCancel = vi.fn();
    const renderer = renderComposer({ onCancel, onSubmit });

    expect(buttonsByLabel(renderer.root, "Stop task")).toHaveLength(1);
    expect(buttonsByLabel(renderer.root, "Send message")).toHaveLength(0);

    inputText(textarea(renderer.root), "steer the current work");

    expect(buttonsByLabel(renderer.root, "Stop task")).toHaveLength(0);
    const sendButton = buttonByLabel(renderer.root, "Send message");
    expect(sendButton.props.disabled).toBe(false);

    click(sendButton);

    expect(onSubmit).toHaveBeenCalledWith("steer the current work");
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("keeps Stop available when an active-task draft is not sendable", () => {
    const onCancel = vi.fn();
    const renderer = renderComposer({ onCancel, submissionAllowed: false });

    inputText(textarea(renderer.root), "Draft for later");

    expect(buttonsByLabel(renderer.root, "Send message")).toHaveLength(0);
    const stopButton = buttonByLabel(renderer.root, "Stop task");
    click(stopButton);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("keeps Stop available while a send attempt is pending", () => {
    const onCancel = vi.fn();
    const renderer = renderComposer({
      onCancel,
      prompt: "Waiting for acceptance",
      submissionAllowed: false,
      submitting: true,
    });

    expect(renderer.root.findByProps({ "aria-label": "Task starting" })).toBeDefined();
    expect(buttonsByLabel(renderer.root, "Send message")).toHaveLength(0);
    const stopButton = buttonByLabel(renderer.root, "Stop task");
    click(stopButton);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("submits from the textarea shortcut only when submit is enabled", () => {
    const onSubmit = vi.fn();
    const preventDefault = vi.fn();
    const renderer = renderComposer({ onSubmit, prompt: "send this" });

    keyDown(textarea(renderer.root), {
      ctrlKey: true,
      preventDefault,
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("send this");

    const disabledSubmit = renderComposer({ onSubmit, submissionAllowed: false });
    keyDown(textarea(disabledSubmit.root), {
      ctrlKey: true,
      preventDefault,
    });

    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("allows attachment-only messages when submission is available", () => {
    const onSubmit = vi.fn();
    const renderer = renderComposer({
      attachments: [attachment("attachment_1", "pasted.png", "attachment-handle-image")],
      onSubmit,
    });

    const sendButton = buttonByLabel(renderer.root, "Send message");
    expect(sendButton.props.disabled).toBe(false);

    click(sendButton);

    expect(onSubmit).toHaveBeenCalledWith("");
  });

  it("enables attachment-only sending only after an attachment is added", () => {
    const emptyRenderer = renderComposer();

    expect(buttonByLabel(emptyRenderer.root, "Send message").props.disabled).toBe(true);

    const attachmentRenderer = renderComposer({
      attachments: [attachment("attachment_1", "pasted.png", "attachment-handle-image")],
    });

    expect(buttonByLabel(attachmentRenderer.root, "Send message").props.disabled).toBe(false);
  });

  it("publishes every editor change to the task-scoped draft owner", () => {
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    const renderer = renderComposer({ onChange, onSubmit });
    const input = textarea(renderer.root);

    inputText(input, "No reducer per key");

    expect(onChange).toHaveBeenCalledWith("No reducer per key");

    click(buttonByLabel(renderer.root, "Send message"));

    expect(onSubmit).toHaveBeenCalledWith("No reducer per key");
  });

  it("does not replace editor markup for ordinary typing", () => {
    const onSubmit = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({ onSubmit, prompt: "seed" });

    expect(editorDom.innerHTML).toBe("seed");

    editorDom.innerHTML = "seed grows";
    inputText(textarea(renderer.root), "seed grows");

    expect(editorDom.innerHTML).toBe("seed grows");

    click(buttonByLabel(renderer.root, "Send message"));

    expect(onSubmit).toHaveBeenCalledWith("seed grows");
    expect(editorDom.innerHTML).toBe("seed grows");
  });

  it("keeps submitted text visible until Backend acceptance clears the authoritative draft", () => {
    const onSubmit = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "" });

    editorDom.innerHTML = "Try reload target again";
    inputText(textarea(renderer.root), "Try reload target again");
    click(buttonByLabel(renderer.root, "Send message"));
    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "Try reload target again", submitting: true }));
    });
    expect(editorDom.innerHTML).toBe("Try reload target again");

    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "", submitting: false }));
    });

    expect(editorDom.innerHTML).toBe("");
  });

  it("clears the local draft when acceptance is batched past a pending render", () => {
    const onSubmit = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({
      commandCatalog: commandCatalog(),
      onCancel: vi.fn(),
      onSubmit,
      prompt: "",
      submissionSettlementKey: 1,
    });

    editorDom.innerHTML = "Accepted immediately";
    inputText(textarea(renderer.root), "Accepted immediately");
    click(buttonByLabel(renderer.root, "Send message"));
    act(() => {
      renderer.update(composerElement({
        commandCatalog: commandCatalog(),
        onCancel: vi.fn(),
        onSubmit,
        prompt: "",
        submissionSettlementKey: 2,
        submitting: false,
      }));
    });

    expect(editorDom.innerHTML).toBe("");
  });

  it("keeps an uncommitted draft through parent refreshes while the Agent is active", () => {
    const onSubmit = vi.fn();
    const renderer = renderComposer({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "" });
    const input = textarea(renderer.root);

    inputText(input, "steer without clearing", 22);
    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "" }));
    });

    click(buttonByLabel(renderer.root, "Send message"));

    expect(onSubmit).toHaveBeenCalledWith("steer without clearing");
  });

  it("does not republish an unchanged draft after a parent refresh", () => {
    const firstOnChange = vi.fn();
    const nextOnChange = vi.fn();
    const renderer = renderComposer({ onChange: firstOnChange, prompt: "" });

    inputText(textarea(renderer.root), "commit through latest callback");
    act(() => {
      renderer.update(composerElement({ onChange: nextOnChange, prompt: "" }));
    });
    expect(firstOnChange).toHaveBeenCalledOnce();
    expect(firstOnChange).toHaveBeenCalledWith("commit through latest callback");
    expect(nextOnChange).not.toHaveBeenCalled();
  });

  it("inserts a visible newline from the configured newline shortcut when Enter sends", () => {
    const onChange = vi.fn();
    const preventDefault = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({
      onChange,
      prompt: "line1",
      submitShortcut: "enter",
    });

    keyDown(textarea(renderer.root), {
      ctrlKey: true,
      currentTarget: {
        selectionEnd: 5,
        selectionStart: 5,
        setRangeText: vi.fn(),
        value: "line1\n",
      },
      preventDefault,
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("line1\n");
    expect(editorDom.innerHTML).toBe("line1<br><br>");
  });

  it("uses Return for a new line instead of submitting on a mobile pointer", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn(() => ({ matches: true })),
    });
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({
      onChange,
      onSubmit,
      prompt: "line1",
      submitShortcut: "enter",
    });

    keyDown(textarea(renderer.root), {
      currentTarget: {
        selectionEnd: 5,
        selectionStart: 5,
        setRangeText: vi.fn(),
        value: "line1\n",
      },
    });

    expect(onSubmit).not.toHaveBeenCalled();
    expect(onChange).toHaveBeenCalledWith("line1\n");
    expect(editorDom.innerHTML).toBe("line1<br><br>");
  });

  it("keeps mobile sending on the visible button when Return has a modifier", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn(() => ({ matches: true })),
    });
    const onSubmit = vi.fn();
    const { renderer } = renderComposerWithEditorDom({
      onSubmit,
      prompt: "line1",
      submitShortcut: "enter",
    });

    keyDown(textarea(renderer.root), { ctrlKey: true });

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("renders disabled composer inputs and controls as disabled", () => {
    const renderer = renderComposer({ attachments: [attachment("attachment_1", "notes.md")], canEdit: false });

    expect(textarea(renderer.root).props["aria-disabled"]).toBe(true);
    expect(buttonByLabel(renderer.root, "Add context").props.disabled).toBe(true);
    expect(buttonByText(renderer.root, "Codex").props.disabled).toBe(true);
    expect(buttonByLabel(renderer.root, "Remove notes.md").props.disabled).toBe(true);
  });

  it("renders the editor enabled when the composer should regain keyboard flow", () => {
    const renderer = renderComposer({ autoFocus: true });

    expect(textarea(renderer.root).props.contentEditable).toBe(true);
  });

  it("does not refocus the composer after sending on a mobile pointer", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn(() => ({ matches: true })),
    });
    const { editorDom, renderer } = renderComposerWithEditorDom({ autoFocus: true });
    const initialFocusCalls = vi.mocked(editorDom.focus).mock.calls.length;

    act(() => {
      renderer.update(composerElement({ autoFocus: true, canEdit: false }));
    });
    act(() => {
      renderer.update(composerElement({ autoFocus: true, canEdit: true }));
    });

    expect(vi.mocked(editorDom.focus)).toHaveBeenCalledTimes(initialFocusCalls);
  });

  it("preserves automatic keyboard flow when the desktop composer becomes available", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn(() => ({ matches: false })),
    });
    const { editorDom, renderer } = renderComposerWithEditorDom({ autoFocus: true });
    const initialFocusCalls = vi.mocked(editorDom.focus).mock.calls.length;

    act(() => {
      renderer.update(composerElement({ autoFocus: true, canEdit: false }));
    });
    act(() => {
      renderer.update(composerElement({ autoFocus: true, canEdit: true }));
    });

    expect(vi.mocked(editorDom.focus).mock.calls.length).toBeGreaterThan(initialFocusCalls);
  });

  it("keeps the empty-state marker that renders the prompt after browser focus", () => {
    const emptyComposer = renderComposer({ autoFocus: true, placeholder: "Describe the task.", prompt: "" });
    const filledComposer = renderComposer({ placeholder: "Describe the task.", prompt: "Review this change" });

    expect(textarea(emptyComposer.root).props["data-empty"]).toBe(true);
    expect(textarea(emptyComposer.root).props["data-placeholder"]).toBe("Describe the task.");
    expect(textarea(emptyComposer.root).props["aria-placeholder"]).toBe("Describe the task.");
    expect(textarea(filledComposer.root).props["data-empty"]).toBeUndefined();
  });

  it("leaves live editor markup outside React ownership so attachment renders preserve the caret", () => {
    const renderer = renderComposer({ prompt: "Explain this screenshot" });

    expect(textarea(renderer.root).props.dangerouslySetInnerHTML).toBeUndefined();
  });

  it("hides the placeholder immediately when the user types before draft synchronization", () => {
    const renderer = renderComposer({ placeholder: "Send follow-up", prompt: "" });
    const toggleAttribute = vi.fn();

    inputText(textarea(renderer.root), "Wtf?", 4, toggleAttribute);

    expect(toggleAttribute).toHaveBeenCalledWith("data-empty", false);
  });

  it("renders composer-level errors", () => {
    const renderer = renderComposer({ error: "Reselect attachments from the file browser before sending." });

    expect(renderer.root.findByProps({ className: "inline-error" }).children).toEqual([
      "Reselect attachments from the file browser before sending.",
    ]);
  });

  it("opens slash command picker with fuzzy results and inserts the selected command", () => {
    const onChange = vi.fn();
    const renderer = renderComposer({
      commandCatalog: commandCatalog(),
      onChange,
    });
    const input = textarea(renderer.root);

    inputText(input, "/$review", "/$review".length);

    const picker = renderer.root.findByProps({ role: "listbox", "aria-label": "Slash commands" });
    expect(text(picker)).toContain("/$doomsday-review");

    click(buttonByText(renderer.root, "/$doomsday-review"));

    expect(onChange).toHaveBeenCalledWith("/$doomsday-review ");
  });

  it("searches workspace files after @ and inserts the selected path as text", async () => {
    vi.useFakeTimers();
    try {
      const onChange = vi.fn();
      const fileBrowser = fileBrowserCallbacks();
      vi.mocked(fileBrowser.searchFiles).mockResolvedValue({
        taskId: "task-1" as never,
        state: "refreshing",
        paths: ["src/main.rs", "docs/team deck.pptx"],
      });
      const renderer = renderComposer({ fileBrowser, onChange });

      inputText(textarea(renderer.root), "Read @ma", 8);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(80);
      });

      expect(fileBrowser.searchFiles).toHaveBeenCalledWith("ma");
      const picker = renderer.root.findByProps({ role: "listbox", "aria-label": "Workspace files" });
      expect(text(picker)).toContain("src/main.rs");
      expect(text(picker)).not.toContain("Refreshing files");
      expect(picker.findByProps({ "data-file-kind": "rust" })).toBeTruthy();
      click(buttonByText(renderer.root, "src/main.rs"));
      expect(onChange).toHaveBeenLastCalledWith("Read @src/main.rs ");
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps prior file results visible while a narrower query is pending", async () => {
    vi.useFakeTimers();
    try {
      const fileBrowser = fileBrowserCallbacks();
      let resolveNarrowSearch: (() => void) | undefined;
      vi.mocked(fileBrowser.searchFiles).mockImplementation(async (query) => {
        if (query === "main") {
          await new Promise<void>((resolve) => {
            resolveNarrowSearch = resolve;
          });
        }
        return { taskId: "task-1" as never, state: "ready", paths: ["src/main.rs"] };
      });
      const renderer = renderComposer({ fileBrowser });

      inputText(textarea(renderer.root), "@ma", 3);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(40);
      });
      expect(text(renderer.root.findByProps({ "aria-label": "Workspace files" }))).toContain("src/main.rs");

      inputText(textarea(renderer.root), "@main", 5);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(40);
      });
      const pendingPicker = renderer.root.findByProps({ "aria-label": "Workspace files" });
      expect(text(pendingPicker)).toContain("src/main.rs");
      expect(text(pendingPicker)).not.toContain("Indexing files");

      await act(async () => resolveNarrowSearch?.());
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens command picker from slash alone and filters skills after dollar", () => {
    const renderer = renderComposer({ commandCatalog: commandCatalog() });
    const input = textarea(renderer.root);

    inputText(input, "/", 1);

    expect(text(renderer.root.findByProps({ role: "listbox", "aria-label": "Slash commands" }))).toContain("/review");

    inputText(input, "/$review", "/$review".length);

    const pickerText = text(renderer.root.findByProps({ role: "listbox", "aria-label": "Slash commands" }));
    expect(pickerText).toContain("/$doomsday-review");
    expect(pickerText).not.toContain("/reviewReview changes.");
  });

  it("opens the slash picker when commands become ready after slash was typed", () => {
    const { editorDom, renderer } = renderComposerWithEditorDom();

    editorDom.innerHTML = "/";
    inputText(textarea(renderer.root), "/", 1);
    expect(renderer.root.findAllByProps({ role: "listbox", "aria-label": "Slash commands" })).toHaveLength(0);

    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog() }));
    });

    expect(text(renderer.root.findByProps({ role: "listbox", "aria-label": "Slash commands" }))).toContain("/review");
  });
});

function renderComposer(overrides: Partial<ComposerTestProps> = {}) {
  let renderer: ReturnType<typeof create> | undefined;
  act(() => {
    renderer = create(composerElement(overrides));
  });
  if (!renderer) throw new Error("Composer renderer was not created");
  return renderer;
}

function renderComposerWithEditorDom(overrides: Partial<ComposerTestProps> = {}) {
  const editorDom = mockEditorDom();
  let renderer: ReturnType<typeof create> | undefined;
  act(() => {
    renderer = create(composerElement(overrides), {
      createNodeMock: (element) =>
        (element.props as { className?: string }).className === "composer-editor" ? editorDom : null,
    });
  });
  if (!renderer) throw new Error("Composer renderer was not created");
  return { editorDom, renderer };
}

function mockEditorDom() {
  let html = "";
  const editor = {
    focus: vi.fn(),
    innerText: "",
    ownerDocument: {
      activeElement: undefined,
      getSelection: () => null,
    },
    textContent: "",
  };
  Object.defineProperty(editor, "innerHTML", {
    get: () => html,
    set: (value: string) => {
      html = value;
      const text = value.replace(/<br>/g, "\n").replace(/<[^>]+>/g, "");
      editor.innerText = text;
      editor.textContent = text;
    },
  });
  return editor as unknown as HTMLElement;
}

function composerElement(overrides: Partial<ComposerTestProps> = {}) {
  const attachments = overrides.attachments ?? [];
  const availability = composerAvailability({
    allowEditingWhileSendBlocked: false,
    attachmentsReady: true,
    connectionStatus: "ready",
    contextReady: true,
    readyPlaceholder: overrides.placeholder ?? "Message",
    sendCapability: { state: "ready" },
    submitPendingLabel: "Task starting",
    submitting: overrides.submitting,
  });
  return (
    <Composer
      agentLocked={overrides.agentLocked}
      agents={overrides.agents}
      attachments={attachments}
      autoFocus={overrides.autoFocus}
      availability={{
        ...availability,
        canEdit: overrides.canEdit ?? availability.canEdit,
        submissionAllowed: overrides.submissionAllowed ?? availability.submissionAllowed,
        submissionBlockedMessage: overrides.submissionBlockedMessage
          ?? availability.submissionBlockedMessage,
      }}
      commandCatalog={overrides.commandCatalog}
      configLocked={overrides.configLocked}
      configOptions={overrides.configOptions}
      error={overrides.error}
      fileBrowser={overrides.fileBrowser}
      imageAttachmentsAllowed={overrides.imageAttachmentsAllowed}
      onCancel={overrides.onCancel}
      onChange={overrides.onChange ?? vi.fn()}
      onUnsupportedImageAttachment={overrides.onUnsupportedImagePaste ?? vi.fn()}
      onRevealAttachment={overrides.onRevealAttachment ?? vi.fn()}
      onRemoveAttachment={overrides.onRemoveAttachment ?? vi.fn()}
      onSelectAgent={overrides.onSelectAgent ?? vi.fn()}
      onSelectConfigOption={overrides.onSelectConfigOption ?? vi.fn()}
      onSelectIsolation={overrides.onSelectIsolation ?? vi.fn()}
      onSubmit={overrides.onSubmit ?? vi.fn()}
      prompt={overrides.prompt ?? ""}
      selection={overrides.selection ?? selection()}
      showAgentSelector={overrides.showAgentSelector}
      showIsolationSelector={overrides.showIsolationSelector}
      submissionSettlementKey={overrides.submissionSettlementKey}
      submitShortcut={overrides.submitShortcut ?? "mod_enter"}
    />
  );
}

type ComposerTestProps = {
  agentLocked: boolean;
  agents: AgentOption[];
  attachments: ComposerAttachment[];
  autoFocus: boolean;
  commandCatalog: AgentCommandsCatalog;
  configLocked: boolean;
  configOptions: ConfigOptionsCatalog;
  canEdit: boolean;
  error: string;
  fileBrowser: TaskFileBrowserCallbacks;
  imageAttachmentsAllowed: boolean;
  onCancel: () => void;
  onChange: (prompt: string) => void;
  onUnsupportedImagePaste: (message?: string) => void;
  onRevealAttachment: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onSelectAgent: (agentId: string) => void;
  onSelectConfigOption: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation: (isolation: ComposerSelection["isolation"]) => void;
  onSubmit: (prompt: string) => void;
  placeholder: string;
  prompt: string;
  selection: ComposerSelection;
  showAgentSelector: boolean;
  showIsolationSelector: boolean;
  submissionAllowed: boolean;
  submissionBlockedMessage: string;
  submitting: boolean;
  submissionSettlementKey: number | string;
  submitShortcut: ComposerSubmitShortcut;
};

function click(instance: ReactTestInstance) {
  act(() => {
    instance.props.onClick();
  });
}

async function clickAsync(instance: ReactTestInstance) {
  await act(async () => {
    await instance.props.onClick();
  });
}

async function settleRenderer() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

function keyDown(instance: ReactTestInstance, overrides: Partial<KeyboardEventProps>) {
  act(() => {
    instance.props.onKeyDown({
      altKey: false,
      ctrlKey: false,
      key: "Enter",
      metaKey: false,
      nativeEvent: { isComposing: false },
      preventDefault: vi.fn(),
      shiftKey: false,
      currentTarget: {
        selectionEnd: 0,
        selectionStart: 0,
        setRangeText: vi.fn(),
        value: "",
      },
      ...overrides,
    });
  });
}

function paste(instance: ReactTestInstance, event: PasteEventProps) {
  act(() => {
    instance.props.onPaste(event);
  });
}

function inputText(
  instance: ReactTestInstance,
  value: string,
  cursor = value.length,
  toggleAttribute = vi.fn(),
) {
  act(() => {
    instance.props.onInput({
      currentTarget: {
        innerText: value,
        ownerDocument: selectionDocument(cursor),
        textContent: value,
        toggleAttribute,
      },
    });
  });
}

function selectionDocument(cursor: number) {
  return {
    createRange: () => ({
      selectNodeContents: vi.fn(),
      setEnd: vi.fn(),
      toString: () => "x".repeat(cursor),
    }),
    getSelection: () => ({
      getRangeAt: () => ({
        endContainer: {},
        endOffset: cursor,
        rangeCount: 1,
        startContainer: {},
        startOffset: cursor,
      }),
      rangeCount: 1,
    }),
  };
}

type KeyboardEventProps = {
  altKey: boolean;
  ctrlKey: boolean;
  key: string;
  metaKey: boolean;
  nativeEvent: { isComposing: boolean };
  preventDefault: () => void;
  shiftKey: boolean;
  currentTarget: {
    selectionEnd: number;
    selectionStart: number;
    setRangeText: (replacement: string, start: number, end: number, selectionMode: SelectionMode) => void;
    value: string;
  };
};

type PasteEventProps = {
  clipboardData: {
    files?: File[];
    items: Array<{ kind: string; type: string; getAsFile: () => File | null }>;
  };
  preventDefault: () => void;
};

function textarea(root: ReactTestInstance) {
  return root.findByProps({ className: "composer-editor" });
}

function buttonByLabel(root: ReactTestInstance, ariaLabel: string) {
  return root.findByProps({ "aria-label": ariaLabel });
}

function buttonsByLabel(root: ReactTestInstance, ariaLabel: string) {
  return root.findAllByProps({ "aria-label": ariaLabel });
}

function menuByLabel(root: ReactTestInstance, ariaLabel: string) {
  return root.findAll((candidate) =>
    (candidate.props.role === "menu" || candidate.props.role === "group")
      && candidate.props["aria-label"] === ariaLabel,
  )[0] ?? missing(ariaLabel);
}

function menusByLabel(root: ReactTestInstance, ariaLabel: string) {
  return root.findAll((candidate) =>
    (candidate.props.role === "menu" || candidate.props.role === "group")
      && candidate.props["aria-label"] === ariaLabel,
  );
}

function buttonByText(root: ReactTestInstance, label: string) {
  return root.findAll((candidate) => candidate.type === "button" && text(candidate).includes(label))[0] ?? missing(label);
}

function buttonsByText(root: ReactTestInstance, label: string) {
  return root.findAll((candidate) => candidate.type === "button" && text(candidate).includes(label));
}

function configControlButtonsByText(root: ReactTestInstance, label: string) {
  return root.findAll((candidate) =>
    candidate.type === "button"
      && String(candidate.props.className ?? "").includes("composer-config-control")
      && text(candidate).includes(label),
  );
}

function menuButtonByStrongLabel(root: ReactTestInstance, label: string) {
  return root.findAll((candidate) =>
    candidate.type === "button" && candidate.findAll((child) => child.type === "strong" && text(child) === label).length > 0,
  )[0] ?? missing(label);
}

function ancestorWithClass(instance: ReactTestInstance, className: string) {
  let ancestor = instance.parent;
  while (ancestor) {
    if (String(ancestor.props.className ?? "").split(/\s+/).includes(className)) return ancestor;
    ancestor = ancestor.parent;
  }
  throw new Error(`Could not find ancestor .${className}`);
}

function menuButtonsByStrongLabel(root: ReactTestInstance, label: string) {
  return root.findAll((candidate) =>
    candidate.type === "button" && candidate.findAll((child) => child.type === "strong" && text(child) === label).length > 0,
  );
}

function missing(label: string): never {
  throw new Error(`Could not find ${label}`);
}

function text(instance: ReactTestInstance): string {
  return instance.findAll(() => true).flatMap((child) => child.children).filter((child) => typeof child === "string").join("");
}

function selection(overrides: Partial<ComposerSelection> = {}): ComposerSelection {
  return {
    agentId: "codex",
    agentLabel: "Codex",
    isolation: "local",
    workspaceLabel: "Workspace",
    workspaceRoot: "/workspace",
    ...overrides,
  };
}

function attachment(localId: string, label: string, handleId?: string, previewUrl?: string): ComposerAttachment {
  return {
    kind: "file",
    label,
    local_id: localId,
    path: handleId ? undefined : `/workspace/${label}`,
    app_server_handle_id: handleId as never,
    preview_url: previewUrl,
  };
}

function configOptions(): ConfigOptionsCatalog {
  return {
    agent_id: "codex",
    options: [
      {
        category: "thought_level",
        kind: "select", current_value: { type: "id", value: "balanced" },
        description: "Reasoning effort.",
        id: "reasoning",
        label: "Reasoning",
        values: [
          { id: "balanced", label: "Balanced", description: "Balanced speed and depth." },
          { id: "high", label: "High", description: "Higher accuracy." },
        ],
      },
    ],
    status: "ready",
  };
}

function booleanConfigOptions(): ConfigOptionsCatalog {
  return {
    agent_id: "codex",
    options: [{
      current_value: { type: "boolean", value: false },
      description: "Skip confirmation prompts.",
      id: "brave_mode",
      kind: "boolean",
      label: "Brave mode",
      values: [],
    }],
    status: "ready",
  };
}

function commandCatalog(): AgentCommandsCatalog {
  return {
    agent_id: "codex",
    commands: [
      { name: "review", description: "Review changes." },
      { name: "$doomsday-review", description: "Strict PR/branch review." },
      { name: "$doomsdayReview", description: "Strict PR/branch review." },
      { name: "$thermo-nuclear-code-quality-review", description: "Maintainability review." },
    ],
    status: "ready",
  };
}

function fileBrowserCallbacks(): TaskFileBrowserCallbacks {
  return {
    ownerKey: "task:test",
    attachEmbedded: vi.fn(async () => undefined),
    attachFileReference: vi.fn(async () => undefined),
    attachImage: vi.fn(async () => undefined),
    searchFiles: vi.fn(async () => ({ taskId: "task-1" as never, state: "ready" as const, paths: [] })),
    listDirectory: vi.fn(async (_rootId: FileBrowserRootId) => ({
      directory: { label: "Workspace", rootId: "root-1" as FileBrowserRootId },
      entries: [
        {
          entryId: "entry-notes" as FileBrowserEntryId,
          kind: "file" as const,
          label: "notes.md",
          selectable: true,
        },
        {
          entryId: "entry-diagram" as FileBrowserEntryId,
          kind: "file" as const,
          label: "diagram.png",
          selectable: true,
        },
      ],
    })),
    listRoots: vi.fn(async () => [{ label: "Workspace", rootId: "root-1" as FileBrowserRootId }]),
  };
}
