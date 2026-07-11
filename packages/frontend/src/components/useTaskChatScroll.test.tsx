import { useState } from "react";
import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import { useTaskChatScroll } from "./useTaskChatScroll";

describe("useTaskChatScroll", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("keeps follow mode off when persisting a small upward scroll", () => {
    const messageList = scrollNode({ clientHeight: 400, scrollHeight: 1400 });
    let tree!: ReactTestRenderer;

    act(() => {
      tree = create(<Harness itemCount={1} />, {
        createNodeMock: (element) => (
          (element.props as { className?: string }).className === "message-list" ? messageList : null
        ),
      });
    });

    messageList.scrollTop = 998;
    act(() => {
      tree.root.findByProps({ className: "message-list" }).props.onScroll({ currentTarget: messageList });
    });

    expect(messageList.scrollTop).toBe(998);
    messageList.scrollHeight = 1500;
    act(() => tree.update(<Harness itemCount={2} />));

    expect(messageList.scrollTop).toBe(998);
  });
});

function Harness({ itemCount }: { itemCount: number }) {
  const [savedScrollTop, setSavedScrollTop] = useState(1000);
  const chatScroll = useTaskChatScroll({
    generating: true,
    itemCount,
    onScrollTop: setSavedScrollTop,
    pendingPrepend: false,
    savedScrollTop,
    taskId: "task_1",
  });

  return (
    <div className="message-list" onScroll={chatScroll.onScroll} ref={chatScroll.messageListRef} />
  );
}

function scrollNode({ clientHeight, scrollHeight }: { clientHeight: number; scrollHeight: number }) {
  let currentScrollHeight = scrollHeight;
  let currentScrollTop = 0;
  return {
    clientHeight,
    get scrollHeight() {
      return currentScrollHeight;
    },
    set scrollHeight(nextScrollHeight: number) {
      currentScrollHeight = nextScrollHeight;
    },
    get scrollTop() {
      return currentScrollTop;
    },
    set scrollTop(nextScrollTop: number) {
      currentScrollTop = Math.max(0, Math.min(nextScrollTop, currentScrollHeight - clientHeight));
    },
  };
}
