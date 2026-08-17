import { isValidElement } from "react";
import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { McpSettingsTab } from "./McpSettingsTab";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("McpSettingsTab", () => {
  it("opens a server and exposes configuration actions", async () => {
    const onLoadServer = vi.fn(async () => ({
      id: "mcp-files",
      label: "Filesystem",
      description: "Approved files",
      enabled: true,
      scope: { kind: "global" as const },
      configuration: {
        transport: "stdio" as const,
        commandLine: "/usr/bin/mcp serve",
        command: "/usr/bin/mcp",
        args: ["serve"],
        secretEnv: ["TOKEN"],
      },
    }));
    let tree: ReturnType<typeof create>;
    await act(async () => {
      tree = create(
        <McpSettingsTab
          availability="available"
          onDeleteServer={() => undefined}
          onLoadServer={onLoadServer}
          onSaveServer={() => undefined}
          onSetEnabled={() => undefined}
          projects={[]}
          servers={[{
            id: "mcp-files",
            label: "Filesystem",
            description: "Approved files",
            enabled: true,
            scope: { kind: "global" },
            transport: "stdio",
            status: "configured",
          }]}
        />,
      );
    });

    const row = tree!.root.findByProps({ "aria-label": "Open Filesystem" });
    await act(async () => row.props.onClick());

    expect(onLoadServer).toHaveBeenCalledWith("mcp-files");
    expect(tree!.root.findAllByType("input").some((input) => input.props.value === "/usr/bin/mcp serve")).toBe(true);
    expect(JSON.stringify(tree!.toJSON())).toContain("Stored securely");
    expect(JSON.stringify(tree!.toJSON())).toContain("Delete");
  });

  it("requires a value when adding a new secret field", async () => {
    const onSaveServer = vi.fn();
    let tree: ReturnType<typeof create>;
    await act(async () => {
      tree = create(
        <McpSettingsTab
          availability="available"
          onDeleteServer={() => undefined}
          onLoadServer={async () => { throw new Error("not used"); }}
          onSaveServer={onSaveServer}
          onSetEnabled={() => undefined}
          projects={[]}
          servers={[]}
        />,
      );
    });

    const addServer = tree!.root.findAllByType("button").find((button) => (
      hasTextChild(button.props.children, "Add server")
    ));
    await act(async () => addServer!.props.onClick());
    const inputs = tree!.root.findAllByType("input");
    await act(async () => {
      inputs.find((input) => input.props.value === "")!.props.onChange({ currentTarget: { value: "Filesystem" } });
    });
    const command = tree!.root.findByProps({ placeholder: "/absolute/path/to/server --arg" });
    await act(async () => command.props.onChange({ currentTarget: { value: "/usr/bin/mcp" } }));
    const addField = tree!.root.findAllByType("button").find((button) => (
      hasTextChild(button.props.children, "Add field")
    ));
    await act(async () => addField!.props.onClick());
    const fieldName = tree!.root.findByProps({ "aria-label": "Environment name" });
    await act(async () => fieldName.props.onChange({ currentTarget: { value: "TOKEN" } }));
    const secret = tree!.root.findByProps({ type: "checkbox" });
    await act(async () => secret.props.onChange({ currentTarget: { checked: true } }));
    const save = tree!.root.findAllByType("button").find((button) => (
      hasTextChild(button.props.children, "Save")
    ));
    await act(async () => save!.props.onClick());

    expect(onSaveServer).not.toHaveBeenCalled();
    expect(JSON.stringify(tree!.toJSON())).toContain("Enter a value for the new secret TOKEN");
  });
});

function hasTextChild(children: unknown, text: string): boolean {
  if (typeof children === "string") return children.includes(text);
  if (Array.isArray(children)) return children.some((child) => hasTextChild(child, text));
  if (isValidElement<{ children?: unknown }>(children)) return hasTextChild(children.props.children, text);
  return false;
}
