import { act, create } from "react-test-renderer";
import type { ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  DesktopUpdateCapability,
  DesktopUpdateSnapshot,
} from "../../services/frontendShell";
import { DesktopUpdateSettings } from "./DesktopUpdateSettings";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("DesktopUpdateSettings", () => {
  it("explains why development builds cannot update", () => {
    const { capability } = fakeCapability({
      revision: 1,
      installedVersion: "1.0.0",
      kind: "unavailable",
      unavailableReason: "developmentBuild",
    });

    const tree = render(capability);

    expect(tree.root.findAllByType("small")[0].children.join(""))
      .toContain("disabled in development builds");
    expect(tree.root.findAllByType("button")).toHaveLength(0);
  });

  it("downloads an available update and renders its trusted metadata", async () => {
    const { capability } = fakeCapability({
      revision: 2,
      installedVersion: "1.0.0",
      kind: "available",
      offer: {
        version: "1.1.0",
        notes: "## Faster startup",
        sizeBytes: 42 * 1024 * 1024,
      },
    });

    const tree = render(capability);
    const download = tree.root.findAllByType("button")
      .find((button) => button.children.includes("Download update"));
    await act(async () => download?.props.onClick());

    expect(capability.download).toHaveBeenCalledOnce();
    expect(tree.root.findByType("summary").children).toContain("Release notes");
    expect(tree.root.findByType("h2").children.join("")).toContain("Faster startup");
  });

  it("opens links in release notes outside the Desktop webview", () => {
    const { capability } = fakeCapability({
      revision: 2,
      installedVersion: "1.0.0",
      kind: "available",
      offer: {
        version: "1.1.0",
        notes: "[Privacy policy](https://github.com/OldKrab/OpenAIDE/blob/main/PRIVACY.md)",
        sizeBytes: 10,
      },
    });

    const tree = render(capability);
    const link = tree.root.findByType("a");

    expect(link.props.href).toBe("https://github.com/OldKrab/OpenAIDE/blob/main/PRIVACY.md");
    expect(link.props.target).toBe("_blank");
  });

  it("reports when the post-update release page cannot open", async () => {
    const { capability } = fakeCapability({
      revision: 3,
      installedVersion: "1.1.0",
      updatedVersion: "1.1.0",
      kind: "idle",
    });
    vi.mocked(capability.openReleaseNotes).mockRejectedValueOnce(new Error("opener rejected URL"));
    const tree = render(capability);
    const viewNotes = tree.root.findAllByType("button")
      .find((button) => button.children.includes("View what's new"));

    await act(async () => viewNotes?.props.onClick());

    expect(tree.root.findByProps({ role: "alert" }).children.join(""))
      .toContain("could not complete");
  });

  it("keeps a ready update available after Not now", () => {
    const { capability } = fakeCapability({
      revision: 3,
      installedVersion: "1.0.0",
      kind: "readyToUpdate",
      offer: { version: "1.1.0", notes: "Fixes", sizeBytes: 10 },
    });
    const tree = render(capability);
    const button = (label: string) => tree.root.findAllByType("button")
      .find((candidate) => candidate.children.includes(label));

    act(() => button("Not now")?.props.onClick());

    expect(button("Show update")).toBeTruthy();
    expect(capability.restartAndUpdate).not.toHaveBeenCalled();
  });

  it("asks before stopping active work for an update", async () => {
    const { capability } = fakeCapability({
      revision: 3,
      installedVersion: "1.0.0",
      kind: "readyToUpdate",
      offer: { version: "1.1.0", notes: "Fixes", sizeBytes: 10 },
    });
    vi.mocked(capability.restartAndUpdate).mockResolvedValueOnce("activeWork").mockResolvedValueOnce("started");
    const tree = render(capability);
    const button = (label: string) => tree.root.findAllByType("button")
      .find((candidate) => candidate.children.includes(label));

    await act(async () => button("Restart and update")?.props.onClick());
    expect(tree.root.findAllByProps({ className: "desktop-update-shutdown-blocker" })[0]
      .children.join(""))
      .toContain("still working");

    await act(async () => button("Stop work and update")?.props.onClick());
    expect(capability.restartAndUpdate).toHaveBeenNthCalledWith(2, { stopActiveWork: true });
  });

  it("ignores stale native snapshots", () => {
    const { capability, publish } = fakeCapability({
      revision: 4,
      installedVersion: "1.0.0",
      kind: "idle",
    });
    const tree = render(capability);

    act(() => publish({
      revision: 3,
      installedVersion: "1.0.0",
      kind: "available",
      offer: { version: "1.1.0", notes: "Stale", sizeBytes: 10 },
    }));

    expect(tree.root.findAllByType("strong")[0].children.join(""))
      .toContain("up to date");
  });
});

function render(capability: DesktopUpdateCapability): ReactTestRenderer {
  let tree!: ReactTestRenderer;
  act(() => {
    tree = create(<DesktopUpdateSettings capability={capability} />);
  });
  return tree;
}

function fakeCapability(initial: DesktopUpdateSnapshot): {
  capability: DesktopUpdateCapability;
  publish(next: DesktopUpdateSnapshot): void;
} {
  let snapshot = initial;
  const listeners = new Set<() => void>();
  const capability: DesktopUpdateCapability = {
    snapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    check: vi.fn(async () => undefined),
    download: vi.fn(async () => undefined),
    cancelDownload: vi.fn(async () => undefined),
    restartAndUpdate: vi.fn(async () => "started" as const),
    openReleaseNotes: vi.fn(async () => undefined),
  };
  return {
    capability,
    publish(next) {
      if (next.revision <= snapshot.revision) return;
      snapshot = next;
      for (const listener of listeners) listener();
    },
  };
}
