import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { describe, expect, it, vi } from "vitest";
import type { UserMessageNavigation } from "./useTaskChatScroll";
import { UserMessageNavigator } from "./UserMessageNavigator";

describe("UserMessageNavigator", () => {
  it("stays absent when one loaded message has no earlier history", () => {
    const navigation = navigationState({
      anchors: [{ key: "message:only", rowIndex: 0, text: "Only question" }],
    });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <UserMessageNavigator navigation={navigation} />,
      );
    });

    expect(tree.toJSON()).toBeNull();
    act(() => tree.unmount());
  });

  it("exposes direct markers, previews, the current position, and paging honesty", () => {
    const navigation = navigationState({
      anchors: [
        { key: "message:first", rowIndex: 1, text: "How should the queue recover?" },
        { key: "message:second", rowIndex: 4, text: "Ship the smallest safe change" },
      ],
      currentIndex: 1,
      hasEarlier: true,
    });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <UserMessageNavigator navigation={navigation} />,
      );
    });

    expect(tree.root.findAllByProps({ "aria-label": "User message navigation" })).toHaveLength(1);
    expect(tree.root.findByProps({
      "aria-label": "User message 1 of 2 loaded: How should the queue recover?",
    })).toBeTruthy();
    expect(tree.root.findByProps({
      "aria-label": "User message 2 of 2 loaded: Ship the smallest safe change",
    }).props["aria-current"]).toBe("true");
    expect(tree.root.findByProps({
      "aria-label": "Load earlier user messages",
    })).toBeTruthy();

    act(() => tree.root.findByProps({
      "aria-label": "User message 1 of 2 loaded: How should the queue recover?",
    }).props.onMouseEnter());
    expect(tree.root.findByProps({ className: "user-message-navigator-preview" }).findByType("p").children)
      .toEqual(["How should the queue recover?"]);

    act(() => tree.root.findByProps({
      "aria-label": "User message 1 of 2 loaded: How should the queue recover?",
    }).props.onClick());
    expect(navigation.navigateTo).toHaveBeenCalledWith(navigation.anchors[0]);
    act(() => tree.unmount());
  });

  it("keeps repeated keyboard navigation on the focused control group", () => {
    const navigation = navigationState({
      anchors: [
        { key: "message:first", rowIndex: 0, text: "First" },
        { key: "message:second", rowIndex: 2, text: "Second" },
      ],
    });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(
        <UserMessageNavigator navigation={navigation} />,
      );
    });
    const rail = tree.root.findByProps({ className: "user-message-navigator" });
    const preventDefault = vi.fn();

    for (const key of ["ArrowUp", "ArrowDown", "Home", "End"]) {
      act(() => rail.props.onKeyDown({
        altKey: false,
        ctrlKey: false,
        key,
        metaKey: false,
        preventDefault,
      }));
    }

    expect(navigation.goPrevious).toHaveBeenCalledTimes(1);
    expect(navigation.goNext).toHaveBeenCalledTimes(1);
    expect(navigation.goFirst).toHaveBeenCalledTimes(1);
    expect(navigation.goLast).toHaveBeenCalledTimes(1);
    expect(preventDefault).toHaveBeenCalledTimes(4);
    act(() => tree.unmount());
  });

  it("expands the mobile rail on demand and collapses it after navigation", () => {
    const navigation = navigationState({
      anchors: [
        { key: "message:first", rowIndex: 0, text: "First" },
        { key: "message:second", rowIndex: 2, text: "Second" },
      ],
    });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<UserMessageNavigator navigation={navigation} />);
    });
    const toggle = tree.root.findByProps({ className: "user-message-navigator-mobile-toggle" });

    expect(toggle.props["aria-expanded"]).toBe(false);
    act(() => toggle.props.onClick());
    expect(toggle.props["aria-expanded"]).toBe(true);

    act(() => tree.root.findByProps({
      "aria-label": "User message 2 of 2: Second",
    }).props.onClick({ preventDefault: vi.fn() }));
    expect(toggle.props["aria-expanded"]).toBe(false);
    expect(navigation.navigateTo).toHaveBeenCalledWith(navigation.anchors[1]);
    act(() => tree.unmount());
  });

  it("keeps a hundred loaded messages directly inspectable without truncating the preview", () => {
    const longMessage = "Large-message detail ".repeat(450);
    const anchors = Array.from({ length: 100 }, (_, index) => ({
      key: `message:${index}`,
      rowIndex: index * 2,
      text: index === 49 ? longMessage : `User message ${index + 1}`,
    }));
    const navigation = navigationState({ anchors, currentIndex: 49 });
    let tree!: ReactTestRenderer;
    act(() => {
      tree = create(<UserMessageNavigator navigation={navigation} />);
    });

    expect(tree.root.findAllByProps({ className: "user-message-position-marker" })).toHaveLength(100);
    expect(tree.root.findByProps({ className: "user-message-navigator-preview" }).findByType("p").children)
      .toEqual([longMessage.trim()]);
    expect(tree.root.findAllByProps({ className: "user-message-position-count" })).toHaveLength(0);
    act(() => tree.unmount());
  });
});

function navigationState(
  overrides: Partial<UserMessageNavigation> = {},
): UserMessageNavigation {
  return {
    anchors: [],
    currentIndex: 0,
    goFirst: vi.fn(),
    goLast: vi.fn(),
    goNext: vi.fn(),
    goPrevious: vi.fn(),
    hasEarlier: false,
    navigateTo: vi.fn(),
    pendingPrevious: false,
    ...overrides,
  };
}
