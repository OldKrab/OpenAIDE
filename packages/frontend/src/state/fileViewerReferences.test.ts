import { describe, expect, it } from "vitest";
import { markdownFileLocation, pathLikeFileLocation, relativeMarkdownHref, chatMarkdownFileLocation } from "./fileViewerReferences";

describe("path-like Agent File References", () => {
  it("opens relative source paths and line suffixes that agents write in inline code", () => {
    expect(pathLikeFileLocation("deploy/local-web.sh")).toEqual({ path: "deploy/local-web.sh" });
    expect(pathLikeFileLocation("tool_details.rs:134")).toEqual({ path: "tool_details.rs", line: 134 });
    expect(pathLikeFileLocation("/home/old/docs")).toEqual({ path: "/home/old/docs" });
  });

  it("does not treat commands, ports, dotted identifiers, or protocol methods as files", () => {
    expect(pathLikeFileLocation("npm run web:dev")).toBeUndefined();
    expect(pathLikeFileLocation("5474")).toBeUndefined();
    expect(pathLikeFileLocation("OPENAIDE_WEB_PORT")).toBeUndefined();
    expect(pathLikeFileLocation("server.listen")).toBeUndefined();
    expect(pathLikeFileLocation(".com")).toBeUndefined();
    expect(pathLikeFileLocation("fileViewer/open")).toBeUndefined();
    expect(pathLikeFileLocation("tool.openPath")).toBeUndefined();
  });
});

describe("markdown file locations", () => {
  it("keeps absolute markdown hrefs including Windows paths", () => {
    expect(markdownFileLocation("/home/dev/app/README.md:12")).toEqual({
      path: "/home/dev/app/README.md",
      line: 12,
    });
    expect(markdownFileLocation("C:/Users/example/project/src/main.ts")).toEqual({
      path: "C:/Users/example/project/src/main.ts",
    });
  });

  it("leaves relative and https hrefs for the File Viewer handle path", () => {
    expect(markdownFileLocation("notes.md")).toBeUndefined();
    expect(relativeMarkdownHref("notes.md")).toBe("notes.md");
    expect(relativeMarkdownHref("https://example.com")).toBeUndefined();
  });

  it("opens Chat markdown links that are workspace-relative file hrefs", () => {
    expect(chatMarkdownFileLocation("deploy/local-web.sh")).toEqual({ path: "deploy/local-web.sh" });
    expect(chatMarkdownFileLocation("README.md:12")).toEqual({ path: "README.md", line: 12 });
    expect(chatMarkdownFileLocation("#L12")).toBeUndefined();
    expect(chatMarkdownFileLocation("https://example.com")).toBeUndefined();
  });
});
