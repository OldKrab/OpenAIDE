import { act, create, type ReactTestInstance } from "react-test-renderer";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { ImagePreviewViewport } from "./ImagePreviewViewport";

const addEventListener = vi.fn();
const imageNodeMock = {
  getBoundingClientRect: () => ({ bottom: 380, height: 360, left: 20, right: 580, top: 20, width: 560 }),
  offsetHeight: 360,
  offsetWidth: 560,
};
const removeEventListener = vi.fn();

describe("ImagePreviewViewport", () => {
  it("zooms out below the fitted size and resets back to fit", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const zoomOut = tree.root.findByProps({ "aria-label": "Zoom image out" });

    expect(zoomOut.props.disabled).toBe(false);
    act(() => zoomOut.props.onClick());

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(0.75)");
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("75%");

    act(() => tree.root.findByProps({ "aria-label": "Reset image zoom" }).props.onClick());

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(1)");
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("Fit");
  });

  it("zooms toward the pointer with the wheel and resets from the zoom control", () => {
    const preventDefault = vi.fn();
    addEventListener.mockClear();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });
    const wheelListener = addEventListener.mock.calls.find(([name]) => name === "wheel")?.[1] as
      | ((event: { preventDefault: () => void }) => void)
      | undefined;

    expect(wheelListener).toBeTypeOf("function");
    wheelListener?.({ preventDefault });
    act(() => stage.props.onWheel({ clientX: 450, clientY: 200, deltaY: -120 }));

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(viewportImage(tree.root).props.style.transform).not.toBe("translate3d(0px, 0px, 0) scale(1)");
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).not.toBe("100%");

    act(() => tree.root.findByProps({ "aria-label": "Reset image zoom" }).props.onClick());

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(1)");
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("Fit");
  });

  it("keeps the visible image center fixed when it differs from the stage center", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: offCenterViewportNodeMock },
      );
    });

    act(() => {
      tree.root.findByProps({ "aria-label": "diagram.png zoomable preview" }).props.onWheel({
        clientX: 260,
        clientY: 180,
        deltaY: -100,
      });
    });

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(1.25)");
  });

  it("applies trackpad wheel zoom from total gesture movement instead of event count", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });

    for (let event = 0; event < 10; event += 1) {
      act(() => {
        tree.root.findByProps({ "aria-label": "diagram.png zoomable preview" }).props.onWheel({
          clientX: 300,
          clientY: 200,
          deltaY: -10,
        });
      });
    }

    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("125%");
  });

  it("pans a zoomed image by dragging without using browser image drag", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });
    const capture = vi.fn();

    act(() => tree.root.findByProps({ "aria-label": "Zoom image in" }).props.onClick());
    act(() => tree.root.findByProps({ "aria-label": "Zoom image in" }).props.onClick());
    act(() => stage.props.onPointerDown(pointerEvent(1, 300, 200, capture)));
    act(() => stage.props.onPointerMove(pointerEvent(1, 340, 225, capture)));

    expect(capture).toHaveBeenCalledWith(1);
    expect(viewportImage(tree.root).props.draggable).toBe(false);
    expect(viewportImage(tree.root).props.style.transform).toContain("translate3d(40px, 25px, 0)");
  });

  it("lets a zoomed image edge reach the canvas center", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });

    act(() => tree.root.findByProps({ "aria-label": "Zoom image in" }).props.onClick());
    act(() => tree.root.findByProps({ "aria-label": "Zoom image in" }).props.onClick());
    act(() => stage.props.onPointerDown(pointerEvent(1, 300, 200)));
    act(() => stage.props.onPointerMove(pointerEvent(1, 900, 800)));

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(420px, 270px, 0) scale(1.5)");
  });

  it("lets a fitted image edge reach the canvas center", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });

    act(() => stage.props.onPointerDown(pointerEvent(1, 300, 200)));
    act(() => stage.props.onPointerMove(pointerEvent(1, 900, 800)));

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(280px, 180px, 0) scale(1)");
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).props.disabled).toBe(false);

    act(() => tree.root.findByProps({ "aria-label": "Reset image zoom" }).props.onClick());

    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(1)");
  });

  it("closes from the empty canvas but not from the image", () => {
    const onClose = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport
          image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }}
          onClose={onClose}
        />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });
    const image = viewportImage(tree.root);

    act(() => stage.props.onClick({ currentTarget: stage, target: image }));
    expect(onClose).not.toHaveBeenCalled();

    act(() => stage.props.onPointerDown({ ...pointerEvent(1, 300, 200), target: imageNodeMock }));
    act(() => stage.props.onPointerUp(pointerEvent(1, 300, 200)));
    act(() => stage.props.onClick({ currentTarget: stage, target: stage }));
    expect(onClose).not.toHaveBeenCalled();

    act(() => stage.props.onClick({ currentTarget: stage, target: stage }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("pinch-zooms and pans around the moving touch midpoint", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });

    act(() => stage.props.onPointerDown(pointerEvent(1, 200, 200, vi.fn(), "touch")));
    act(() => stage.props.onPointerDown(pointerEvent(2, 400, 200, vi.fn(), "touch")));
    act(() => stage.props.onPointerMove(pointerEvent(2, 500, 200, vi.fn(), "touch")));

    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("150%");
    expect(viewportImage(tree.root).props.style.transform).not.toContain("translate3d(0px, 0px, 0)");
  });

  it("supports double-click zoom and keyboard reset", () => {
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <ImagePreviewViewport image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }} />,
        { createNodeMock: viewportNodeMock },
      );
    });
    const stage = tree.root.findByProps({ className: "attachment-preview-stage" });
    const preventDefault = vi.fn();

    act(() => stage.props.onDoubleClick({ clientX: 300, clientY: 200, preventDefault }));
    expect(tree.root.findByProps({ "aria-label": "Reset image zoom" }).children.join("")).toBe("200%");

    act(() => stage.props.onKeyDown({ key: "0", preventDefault }));
    expect(preventDefault).toHaveBeenCalledTimes(2);
    expect(viewportImage(tree.root).props.style.transform).toBe("translate3d(0px, 0px, 0) scale(1)");
  });
});

function viewportImage(root: ReactTestInstance) {
  return root.findByProps({ className: "attachment-preview-image" });
}

function viewportNodeMock(element: ReactElement) {
  const props = element.props as Record<string, unknown>;
  if (element.type === "div" && props.className === "attachment-preview-stage") {
    return {
      addEventListener,
      clientHeight: 400,
      clientWidth: 600,
      getBoundingClientRect: () => ({ bottom: 400, height: 400, left: 0, right: 600, top: 0, width: 600 }),
      releasePointerCapture: vi.fn(),
      removeEventListener,
      setPointerCapture: vi.fn(),
    };
  }
  if (element.type === "img") return imageNodeMock;
  return null;
}

function offCenterViewportNodeMock(element: ReactElement) {
  if (element.type === "img") {
    return {
      ...imageNodeMock,
      getBoundingClientRect: () => ({ bottom: 360, height: 360, left: -20, right: 540, top: 0, width: 560 }),
    };
  }
  return viewportNodeMock(element);
}

function pointerEvent(
  pointerId: number,
  clientX: number,
  clientY: number,
  setPointerCapture = vi.fn(),
  pointerType = "mouse",
) {
  return {
    clientX,
    clientY,
    currentTarget: { releasePointerCapture: vi.fn(), setPointerCapture },
    button: 0,
    pointerId,
    pointerType,
    preventDefault: vi.fn(),
  };
}
