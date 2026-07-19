import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ExecuteToolDetails } from "./ExecuteToolDetails";

describe("ExecuteToolDetails", () => {
  it("renders equal stdout and stderr as independently labeled channels", () => {
    const html = renderToStaticMarkup(
      <ExecuteToolDetails
        details={{
          locations: [],
          content: [],
          input: { command: ["check"], fields: [] },
          output: { stdout: "same output", stderr: "same output", fields: [] },
        }}
        step={{ kind: "tool", name: "execute", status: "completed" }}
      />,
    );

    expect(html).toContain("<span>stdout</span><pre>same output</pre>");
    expect(html).toContain("<span>stderr</span><pre>same output</pre>");
    expect(html.match(/same output/g)).toHaveLength(2);
  });

  it.each([
    ["running", "Running", "lucide-loader-circle"],
    ["completed", "Completed", "lucide-check"],
    ["interrupted", "Interrupted", "lucide-x"],
    ["error", "Failed", "lucide-x"],
    ["future_status", "Unknown", "lucide-x"],
  ])("renders authoritative %s result text and icon", (status, label, icon) => {
    const html = renderToStaticMarkup(
      <ExecuteToolDetails
        details={{
          locations: [],
          content: [],
          input: { command: ["npm", "test"], fields: [] },
          output: {
            stdout: "out",
            stderr: "err",
            exit_code: 7,
            success: status === "completed",
            fields: [{ name: "duration", value: { kind: "string", value: "1.25s" } }],
          },
        }}
        step={{ kind: "tool", name: "execute", status: status as never }}
      />,
    );

    expect(html).toContain(label);
    expect(html).toContain(icon);
    expect(html).toContain("stdout");
    expect(html).toContain("stderr");
    expect(html).toContain("exit 7");
    expect(html).toContain("1.25s");
    if (status === "running") expect(html).not.toContain("Completed");
  });
});
