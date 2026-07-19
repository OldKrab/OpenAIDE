import { act, create } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import { AgentRecoveryPanel, agentRecoveryKind, NODE_JS_DOWNLOAD_URL } from "./AgentRecovery";

describe("Agent recovery", () => {
  it("uses the App Server's structured Node.js setup reason", () => {
    expect(agentRecoveryKind({
      id: "codex",
      label: "Codex",
      description: "",
      icon: "openai",
      status: "setupRequired",
      setupReason: "nodeJsRequired",
    })).toBe("nodeJsRequired");
  });

  it("renders product copy and explicit Node.js recovery actions", () => {
    const onOpenExternal = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentRecoveryPanel
        actions={{
          onOpenAgentSettings: vi.fn(),
          onOpenExternal,
          onRetry: vi.fn(async () => false),
        }}
        agent={{ id: "codex", label: "Codex" }}
        kind="nodeJsRequired"
      />);
    });

    expect(textContent(tree)).toContain("Codex needs Node.js");
    expect(textContent(tree)).toContain("OpenAIDE can't access the Node.js tools required to start Codex.");
    act(() => button(tree, "Install Node.js").props.onClick());
    expect(onOpenExternal).toHaveBeenCalledWith(NODE_JS_DOWNLOAD_URL);
  });

  it("opens authentication settings with the preserved New Task return intent", () => {
    const onOpenAgentSettings = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<AgentRecoveryPanel
        actions={{ onOpenAgentSettings, onOpenExternal: vi.fn(), onRetry: vi.fn(async () => true) }}
        agent={{ id: "codex", label: "Codex" }}
        kind="authRequired"
        returnToNewTask
      />);
    });

    act(() => button(tree, "Choose sign-in method").props.onClick());
    expect(onOpenAgentSettings).toHaveBeenCalledWith("codex", true);
  });
});

function textContent(tree: ReturnType<typeof create>) {
  return tree.root.findAllByType("strong").concat(tree.root.findAllByType("small"))
    .map((node) => node.children.join(""))
    .join(" ");
}

function button(tree: ReturnType<typeof create>, label: string) {
  return tree.root.findAllByType("button").find((node) => node.children.join("") === label)!;
}
