import { describe, expect, it } from "vitest";
import {
  chatMarkdownFileLocation,
  inlineFileReferenceCandidate,
  markdownFileLocation,
  pathLikeFileLocation,
  relativeMarkdownHref,
} from "./fileViewerReferences";

describe("path-like Agent File References", () => {
  it("opens relative source paths and line suffixes that agents write in inline code", () => {
    expect(pathLikeFileLocation("deploy/local-web.sh")).toEqual({ path: "deploy/local-web.sh" });
    expect(pathLikeFileLocation("tool_details.rs:134")).toEqual({ path: "tool_details.rs", line: 134 });
    expect(pathLikeFileLocation("/home/user/docs")).toEqual({ path: "/home/user/docs" });
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

describe("inline file reference candidates", () => {
  it("offers any relative file type for checking and takes absolute paths as written", () => {
    expect(inlineFileReferenceCandidate("cv/resume_v2.pdf")).toEqual({ location: { path: "cv/resume_v2.pdf" }, needsCheck: true });
    expect(inlineFileReferenceCandidate("./notes.typ:3")).toEqual({ location: { path: "notes.typ", line: 3 }, needsCheck: true });
    expect(inlineFileReferenceCandidate("/srv/app/run.log")).toEqual({ location: { path: "/srv/app/run.log" }, needsCheck: false });
  });

  it("skips words, commands, URLs and directories", () => {
    expect(inlineFileReferenceCandidate("useState")).toBeUndefined();
    expect(inlineFileReferenceCandidate("npm run build")).toBeUndefined();
    expect(inlineFileReferenceCandidate("https://example.com/a.pdf")).toBeUndefined();
    expect(inlineFileReferenceCandidate("docs/")).toBeUndefined();
    expect(inlineFileReferenceCandidate("..")).toBeUndefined();
  });
});
