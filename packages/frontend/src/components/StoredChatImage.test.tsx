import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentMessagePart, Attachment, ChatMessage } from "@openaide/app-shell-contracts";

const IMAGE_ID = "f".repeat(64);
const IMAGE_DATA = "aW1hZ2U=";
const IMAGE_URL = `data:image/png;base64,${IMAGE_DATA}`;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

function userMessage(id: string, text: string, attachments: Attachment[]): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_type: "user",
    message_id: id,
    message: { kind: "user", id, text, created_at: "2026-10-01T00:00:00Z", attachments },
  };
}

function agentImageMessage(id: string): ChatMessage {
  const parts: AgentMessagePart[] = [
    { kind: "text", text: "Here is the screenshot." },
    { kind: "image", media_type: "image/png", image_id: IMAGE_ID, size_bytes: 5 },
  ];
  return {
    cursor: id,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: { kind: "agent_message", id, role: "agent", parts, created_at: "2026-10-01T00:00:00Z" },
  };
}

const storedImage: Attachment = {
  kind: "image",
  label: "Image",
  payload: { imageId: IMAGE_ID, mimeType: "image/png", sizeBytes: 5 },
};

async function renderRows(messages: ChatMessage[], loadImage: (taskId: string, imageId: string) => Promise<string>) {
  const { ChatRow } = await import("./ChatMessageView");
  const { ChatImageProvider } = await import("./StoredChatImage");
  const { createChatImageCache } = await import("../state/chatImageCache");
  let tree!: ReactTestRenderer;
  await act(async () => {
    tree = create(
      <ChatImageProvider cache={createChatImageCache()} loadImage={loadImage} taskId="task_1">
        {messages.map((message) => (
          <ChatRow key={message.message_id} message={message} onPermissionRespond={vi.fn()} taskId="task_1" />
        ))}
      </ChatImageProvider>,
    );
  });
  return tree;
}

describe("stored Chat images", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.resetModules();
    vi.stubGlobal("window", { acquireVsCodeApi: undefined, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows a loading frame beside the message text, then the image that opens in the lightbox", async () => {
    const response = deferred<string>();
    const loadImage = vi.fn(() => response.promise);
    const tree = await renderRows([userMessage("u1", "What is this?", [storedImage])], loadImage);

    expect(loadImage).toHaveBeenCalledWith("task_1", IMAGE_ID);
    expect(tree.root.findByProps({ "aria-label": "Loading Image" }).props["aria-busy"]).toBe("true");
    expect(JSON.stringify(tree.toJSON())).toContain("What is this?");
    expect(tree.root.findAllByProps({ "aria-label": "Open Image" })).toHaveLength(0);

    await act(async () => response.resolve(IMAGE_DATA));

    expect(tree.root.findAllByProps({ "aria-label": "Loading Image" })).toHaveLength(0);
    const open = tree.root.findByProps({ "aria-label": "Open Image" });
    expect(open.findByType("img").props.src).toBe(IMAGE_URL);
    await act(async () => open.props.onClick());
    const dialog = tree.root.findByProps({ role: "dialog" });
    expect(dialog.findByProps({ alt: "Image" }).props.src).toBe(IMAGE_URL);
    await act(async () => tree.unmount());
  });

  it("shows an unavailable state with Retry that requests the image again", async () => {
    const loadImage = vi.fn()
      .mockImplementationOnce(async () => { throw new Error("notFound"); })
      .mockImplementationOnce(async () => IMAGE_DATA);
    const tree = await renderRows([agentImageMessage("a1")], loadImage);

    expect(tree.root.findByProps({ "aria-label": "Agent image unavailable" })).toBeDefined();
    expect(JSON.stringify(tree.toJSON())).toContain("Image unavailable");
    expect(JSON.stringify(tree.toJSON())).toContain("Here is the screenshot.");

    await act(async () => tree.root.findByProps({ "aria-label": "Retry loading Agent image" }).props.onClick());

    expect(loadImage).toHaveBeenCalledTimes(2);
    expect(tree.root.findAllByProps({ "aria-label": "Agent image unavailable" })).toHaveLength(0);
    expect(tree.root.findByProps({ "aria-label": "Open Agent image" }).findByType("img").props.src).toBe(IMAGE_URL);
    await act(async () => tree.unmount());
  });

  it("shares one request among every view of the same image", async () => {
    const response = deferred<string>();
    const loadImage = vi.fn(() => response.promise);
    const tree = await renderRows([
      userMessage("u1", "first", [storedImage]),
      userMessage("u2", "second", [storedImage]),
      agentImageMessage("a1"),
    ], loadImage);

    await act(async () => response.resolve(IMAGE_DATA));

    expect(loadImage).toHaveBeenCalledOnce();
    expect(tree.root.findAllByProps({ "aria-label": "Open Image" })).toHaveLength(2);
    expect(tree.root.findAllByProps({ "aria-label": "Open Agent image" })).toHaveLength(1);
    await act(async () => tree.unmount());
  });

  it("renders a local preview the client still holds without loading anything", async () => {
    const loadImage = vi.fn(async () => IMAGE_DATA);
    const tree = await renderRows([userMessage("u1", "pending", [
      { kind: "image", label: "local.png", payload: { previewUrl: IMAGE_URL } },
    ])], loadImage);

    expect(tree.root.findByProps({ "aria-label": "Open local.png" }).findByType("img").props.src).toBe(IMAGE_URL);
    expect(loadImage).not.toHaveBeenCalled();
    await act(async () => tree.unmount());
  });
});
