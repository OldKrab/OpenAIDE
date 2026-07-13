import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";

const markdownRender = vi.hoisted(() => vi.fn());

vi.mock("react-markdown", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-markdown")>();
  return {
    ...actual,
    default: ({ children }: { children: string }) => {
      markdownRender(children);
      return <p>{children}</p>;
    },
  };
});

import { AgentMarkdown } from "./AgentMarkdown";

describe("AgentMarkdown render isolation", () => {
  beforeEach(() => {
    markdownRender.mockClear();
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("does not parse unchanged message text again when its parent renders", () => {
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<AgentMarkdown className="chat-agent" text="A stable **answer**" />);
    });

    act(() => {
      tree.update(<AgentMarkdown className="chat-agent" text="A stable **answer**" />);
    });

    expect(markdownRender).toHaveBeenCalledOnce();
  });

  it("parses a message again when streamed text changes", () => {
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<AgentMarkdown className="chat-agent" streaming text="Partial" />);
    });

    act(() => {
      tree.update(<AgentMarkdown className="chat-agent" streaming text="Partial response" />);
    });

    expect(markdownRender).toHaveBeenCalledTimes(2);
    expect(markdownRender).toHaveBeenLastCalledWith("Partial response");
  });
});
