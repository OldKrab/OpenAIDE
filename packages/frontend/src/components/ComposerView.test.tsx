import { act, create, type ReactTestInstance } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentCommandsCatalog, ComposerSubmitShortcut, ConfigOptionsCatalog } from "@openaide/app-shell-contracts";
import type { FileBrowserEntryId, FileBrowserRootId } from "@openaide/app-server-client";
import type { AgentOption, ComposerAttachment, ComposerSelection } from "../state/composerOptions";
import { Composer } from "./Composer";
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
    const imageGrid = renderer.root.findByProps({ className: "composer-image-grid", "data-layout": "single" });
    const composerHtml = JSON.stringify(renderer.toJSON());

    expect(preview.props.src).toBe("data:image/png;base64,aW1hZ2U=");
    expect(preview.props.alt).toBe("Clipboard image preview");
    expect(imageGrid).toBeTruthy();
    expect(composerHtml.indexOf("composer-image-grid")).toBeLessThan(composerHtml.indexOf("composer-editor"));
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

    expect(renderer.root.findByProps({ className: "composer-image-grid", "data-layout": "many" })).toBeTruthy();
    expect(renderer.root.findAllByProps({ className: "composer-image-preview" })).toHaveLength(3);
    expect(renderer.root.findAllByProps({ className: "composer-image-remove" })).toHaveLength(3);
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
      disabled: true,
    });

    click(buttonByLabel(renderer.root, "Open Clipboard image"));

    const lightbox = renderer.root.findByProps({ className: "attachment-preview-lightbox" });
    expect(text(lightbox)).not.toContain("Clipboard image");
    expect(renderer.root.findByProps({ className: "attachment-preview-stage" }).findByType("img").props.src).toBe(
      "data:image/png;base64,aW1hZ2U=",
    );
  });

  it("offers only project files and device uploads in the add-context menu", () => {
    const renderer = renderComposer({ selection: selection({ workspaceRoot: "", workspaceLabel: "Workspace" }) });

    click(buttonByLabel(renderer.root, "Add context"));

    const menu = menuByLabel(renderer.root, "Add context");
    expect(menu.findAllByProps({ role: "menuitem" })).toHaveLength(2);
    expect(text(menuButtonByStrongLabel(renderer.root, "Project files"))).toContain("Browse files and images in this project.");
    expect(text(menuButtonByStrongLabel(renderer.root, "Upload or photo"))).toContain("Choose images from this device.");
    expect(menuButtonByStrongLabel(renderer.root, "Project files").props.disabled).toBe(true);
    expect(menuButtonByStrongLabel(renderer.root, "Upload or photo").props.disabled).toBe(true);
    expect(renderer.root.findAllByProps({ type: "file" })[0].props.disabled).toBe(true);
  });

  it("browses project files and attaches either a live reference or a snapshot", async () => {
    const fileBrowser = fileBrowserCallbacks();
    const renderer = renderComposer({ fileBrowser });

    click(buttonByLabel(renderer.root, "Add context"));
    click(menuButtonByStrongLabel(renderer.root, "Project files"));
    await settleRenderer();

    expect(fileBrowser.listRoots).toHaveBeenCalledTimes(1);
    expect(fileBrowser.listDirectory).toHaveBeenCalledWith("root-1");
    expect(text(menuByLabel(renderer.root, "Project files"))).toContain("notes.md");
    expect(text(menuByLabel(renderer.root, "Project files"))).toContain("diagram.png");

    click(buttonByText(renderer.root, "Reference"));
    await settleRenderer();

    expect(fileBrowser.attachFileReference).toHaveBeenCalledWith("entry-notes");
    expect(menusByLabel(renderer.root, "Project files")).toHaveLength(0);

    click(buttonByLabel(renderer.root, "Add context"));
    click(menuButtonByStrongLabel(renderer.root, "Project files"));
    await settleRenderer();
    const diagramRow = renderer.root
      .findAllByProps({ className: "composer-file-row file" })
      .find((row) => text(row).includes("diagram.png"));
    expect(diagramRow).toBeTruthy();
    click(buttonByText(diagramRow!, "Embed"));
    await settleRenderer();

    expect(fileBrowser.attachEmbedded).toHaveBeenCalledWith("entry-diagram");
    expect(menusByLabel(renderer.root, "Project files")).toHaveLength(0);
  });

  it("closes the project file picker on Escape and click-away inside the composer", async () => {
    const fileBrowser = fileBrowserCallbacks();
    const renderer = renderComposer({ fileBrowser });

    click(buttonByLabel(renderer.root, "Add context"));
    click(menuButtonByStrongLabel(renderer.root, "Project files"));
    await settleRenderer();

    expect(menuByLabel(renderer.root, "Project files")).toBeTruthy();

    act(() => {
      renderer.root.findByType("section").props.onKeyDown({ key: "Escape" });
    });
    expect(menusByLabel(renderer.root, "Project files")).toHaveLength(0);

    click(buttonByLabel(renderer.root, "Add context"));
    click(menuButtonByStrongLabel(renderer.root, "Project files"));
    await settleRenderer();
    expect(menuByLabel(renderer.root, "Project files")).toBeTruthy();

    act(() => {
      textarea(renderer.root).props.onPointerDown();
    });
    expect(menusByLabel(renderer.root, "Project files")).toHaveLength(0);
  });

  it("uploads every selected image through the App Server attachment callback", async () => {
    const first = new File(["first"], "first.png", { type: "image/png" });
    const second = new File(["second"], "second.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    const renderer = renderComposer({ fileBrowser });

    click(buttonByLabel(renderer.root, "Add context"));

    const uploadButton = menuButtonByStrongLabel(renderer.root, "Upload or photo");
    expect(uploadButton.props.disabled).toBeFalsy();
    const input = renderer.root.findByProps({ type: "file" });
    expect(input.props.multiple).toBe(true);
    act(() => {
      input.props.onChange({
        currentTarget: { value: "upload.png" },
        target: { files: [first, second] },
      });
    });
    await settleRenderer();

    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(1, first);
    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(2, second);
    expect(menusByLabel(renderer.root, "Add context")).toHaveLength(0);
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
    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(1, first, {
      prompt: "Explain this screenshot",
      context: [],
    });
    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(2, second, {
      prompt: "Explain this screenshot",
      context: [],
    });
  });

  it("continues attaching later pasted images when one image fails", async () => {
    const first = new File(["first"], "first.png", { type: "image/png" });
    const second = new File(["second"], "second.png", { type: "image/png" });
    const fileBrowser = fileBrowserCallbacks();
    fileBrowser.attachPastedImage = vi.fn(async (file) => {
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

    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(1, first, { prompt: "", context: [] });
    expect(fileBrowser.attachPastedImage).toHaveBeenNthCalledWith(2, second, { prompt: "", context: [] });
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
    expect(fileBrowser.attachPastedImage).toHaveBeenCalledWith(file, {
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
    expect(text(menuByLabel(renderer.root, "Reasoning"))).toContain("Higher accuracy.");
    click(menuButtonByStrongLabel(renderer.root, "High"));
    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", "high");

    click(buttonByText(renderer.root, "Worktree"));
    const worktreeButton = menuButtonByStrongLabel(renderer.root, "Worktree");
    expect(worktreeButton.props["aria-checked"]).toBe(true);
    click(menuButtonByStrongLabel(renderer.root, "Docker"));
    expect(onSelectIsolation).toHaveBeenCalledWith("docker");
  });

  it("offers run configuration through the compact Options menu", () => {
    const configOptionsWithModel: ConfigOptionsCatalog = {
      agent_id: "codex",
      options: [
        {
          category: "model",
          current_value: "gpt-5.5",
          id: "model",
          label: "Model",
          values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
        },
        ...configOptions().options,
      ],
      status: "ready",
    };
    const renderer = renderComposer({
      configOptions: configOptionsWithModel,
      selection: selection({ isolation: "git_worktree" }),
    });

    click(buttonByText(renderer.root, "Options · GPT-5.5"));

    expect(text(menuByLabel(renderer.root, "Run options"))).toContain("Model");
    expect(text(menuByLabel(renderer.root, "Run options"))).toContain("Reasoning");
    expect(text(menuByLabel(renderer.root, "Run options"))).toContain("Isolation");

    click(menuButtonByStrongLabel(renderer.root, "Reasoning"));

    expect(text(menuByLabel(renderer.root, "Reasoning"))).toContain("Higher accuracy.");
    const backButton = renderer.root.findByProps({ className: "composer-popover-back" });
    expect(backButton.props["aria-label"]).toBe("Back to options");
    expect(text(backButton)).toBe("Reasoning");

    click(backButton);

    expect(text(menuByLabel(renderer.root, "Run options"))).toContain("Model");
  });

  it("labels mode config controls with the compact selected value", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "mode",
            current_value: "agent",
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
            current_value: "agent",
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
            current_value: "gpt-5.5",
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

  it("uses short setting prefixes for ambiguous config values", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "mode",
            current_value: "agent",
            id: "mode",
            label: "Mode",
            values: [{ id: "agent", label: "Agent" }],
          },
          {
            current_value: "off",
            id: "fast",
            label: "Fast mode",
            values: [{ id: "off", label: "Off" }],
          },
        ],
        status: "ready",
      },
    });

    expect(configControlButtonsByText(renderer.root, "Agent")).toHaveLength(1);
    expect(configControlButtonsByText(renderer.root, "Fast: Off")).toHaveLength(1);
  });

  it("positions a configuration menu from the control that opened it", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            current_value: "off",
            id: "fast",
            label: "Fast mode",
            values: [{ id: "off", label: "Off" }],
          },
        ],
        status: "ready",
      },
    });

    click(configControlButtonsByText(renderer.root, "Fast: Off")[0]);

    const menuAnchor = menuByLabel(renderer.root, "Fast mode").parent?.parent;
    expect(menuAnchor?.props.className).toContain("composer-option-anchor");
    expect(text(menuAnchor as ReactTestInstance)).toContain("Fast: Off");
  });

  it("normalizes lowercase config value labels in compact controls", () => {
    const renderer = renderComposer({
      configOptions: {
        agent_id: "codex",
        options: [
          {
            category: "thought_level",
            current_value: "medium",
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
    expect(onSelectConfigOption).toHaveBeenCalledWith("reasoning", "high");
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

    const sendRenderer = renderComposer({ onSubmit, submitDisabled: true });
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

    const disabledSubmit = renderComposer({ onSubmit, submitDisabled: true });
    keyDown(textarea(disabledSubmit.root), {
      ctrlKey: true,
      preventDefault,
    });

    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("blocks attachment-only messages when the Agent requires text", () => {
    const onSubmit = vi.fn();
    const renderer = renderComposer({
      attachments: [attachment("attachment_1", "pasted.png", "attachment-handle-image")],
      onSubmit,
    });

    const sendButton = buttonByLabel(renderer.root, "Send message");
    expect(sendButton.props.disabled).toBe(true);

    click(sendButton);

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("keeps keystrokes out of React callbacks and submits the DOM draft", () => {
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    const renderer = renderComposer({ onChange, onSubmit });
    const input = textarea(renderer.root);

    inputText(input, "No reducer per key");

    expect(onChange).not.toHaveBeenCalled();

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

  it("clears the local draft after Backend acceptance settles submission", () => {
    const onSubmit = vi.fn();
    const { editorDom, renderer } = renderComposerWithEditorDom({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "" });

    editorDom.innerHTML = "Try reload target again";
    inputText(textarea(renderer.root), "Try reload target again");
    click(buttonByLabel(renderer.root, "Send message"));
    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "", submitPending: true }));
    });
    act(() => {
      renderer.update(composerElement({ commandCatalog: commandCatalog(), onCancel: vi.fn(), onSubmit, prompt: "", submitPending: false }));
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

  it("keeps composer callbacks fresh when a parent refresh leaves editor markup unchanged", () => {
    const firstOnChange = vi.fn();
    const nextOnChange = vi.fn();
    const renderer = renderComposer({ onChange: firstOnChange, prompt: "" });

    inputText(textarea(renderer.root), "commit through latest callback");
    act(() => {
      renderer.update(composerElement({ onChange: nextOnChange, prompt: "" }));
    });
    textarea(renderer.root).props.onBlur();

    expect(firstOnChange).not.toHaveBeenCalled();
    expect(nextOnChange).toHaveBeenCalledWith("commit through latest callback");
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
    expect(onChange).not.toHaveBeenCalled();
    expect(editorDom.innerHTML).toBe("line1<br><br>");
  });

  it("renders disabled composer inputs and controls as disabled", () => {
    const renderer = renderComposer({ attachments: [attachment("attachment_1", "notes.md")], disabled: true });

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
      renderer.update(composerElement({ autoFocus: true, disabled: true }));
    });
    act(() => {
      renderer.update(composerElement({ autoFocus: true, disabled: false }));
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
      renderer.update(composerElement({ autoFocus: true, disabled: true }));
    });
    act(() => {
      renderer.update(composerElement({ autoFocus: true, disabled: false }));
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
  return (
    <Composer
      agentLocked={overrides.agentLocked}
      agents={overrides.agents}
      attachments={overrides.attachments ?? []}
      autoFocus={overrides.autoFocus}
      commandCatalog={overrides.commandCatalog}
      configLocked={overrides.configLocked}
      configOptions={overrides.configOptions}
      disabled={overrides.disabled ?? false}
      error={overrides.error}
      fileBrowser={overrides.fileBrowser}
      onCancel={overrides.onCancel}
      onChange={overrides.onChange ?? vi.fn()}
      onUnsupportedImageAttachment={overrides.onUnsupportedImagePaste ?? vi.fn()}
      onRevealAttachment={overrides.onRevealAttachment ?? vi.fn()}
      onRemoveAttachment={overrides.onRemoveAttachment ?? vi.fn()}
      onSelectAgent={overrides.onSelectAgent ?? vi.fn()}
      onSelectConfigOption={overrides.onSelectConfigOption ?? vi.fn()}
      onSelectIsolation={overrides.onSelectIsolation ?? vi.fn()}
      onSubmit={overrides.onSubmit ?? vi.fn()}
      placeholder={overrides.placeholder ?? "Message"}
      prompt={overrides.prompt ?? ""}
      selection={overrides.selection ?? selection()}
      showAgentSelector={overrides.showAgentSelector}
      showIsolationSelector={overrides.showIsolationSelector}
      submitDisabled={overrides.submitDisabled ?? false}
      submitPending={overrides.submitPending}
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
  disabled: boolean;
  error: string;
  fileBrowser: TaskFileBrowserCallbacks;
  onCancel: () => void;
  onChange: (prompt: string) => void;
  onUnsupportedImagePaste: (message?: string) => void;
  onRevealAttachment: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onSelectAgent: (agentId: string) => void;
  onSelectConfigOption: (configId: string, value: string) => void;
  onSelectIsolation: (isolation: ComposerSelection["isolation"]) => void;
  onSubmit: (prompt: string) => void;
  placeholder: string;
  prompt: string;
  selection: ComposerSelection;
  showAgentSelector: boolean;
  showIsolationSelector: boolean;
  submitDisabled: boolean;
  submitPending: boolean;
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
  return root.findByProps({ role: "menu", "aria-label": ariaLabel });
}

function menusByLabel(root: ReactTestInstance, ariaLabel: string) {
  return root.findAllByProps({ role: "menu", "aria-label": ariaLabel });
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
    configOptions: {},
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
        current_value: "balanced",
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
    attachEmbedded: vi.fn(async () => undefined),
    attachFileReference: vi.fn(async () => undefined),
    attachPastedImage: vi.fn(async () => undefined),
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
