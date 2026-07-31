// @vitest-environment jsdom

import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it } from "vitest";
import { PrototypeView } from "./PrototypeView";
import type { PrototypeDefinition } from "./prototypeApi";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("prototype variant switching", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/prototype/example/?variant=A");
  });

  it("switches the rendered variant and shareable URL without leaving the React view", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<PrototypeView definition={definition} />);
    });

    expect(tree.root.findByType("article").children).toEqual(["Variant A"]);

    act(() => tree.root.findByProps({ "aria-label": "Next variant" }).props.onClick());

    expect(tree.root.findByType("article").children).toEqual(["Variant B"]);
    expect(window.location.search).toBe("?variant=B");
  });
});

const definition: PrototypeDefinition = {
  title: "Example",
  question: "Which structure works better?",
  variants: [
    { key: "A", name: "First", Component: () => <article>Variant A</article> },
    { key: "B", name: "Second", Component: () => <article>Variant B</article> },
  ],
};
