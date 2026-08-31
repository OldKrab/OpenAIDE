# User-Initiated Arbitrary File Opening

Status: accepted

When a user explicitly activates an Agent File Reference, Web and Desktop may open any absolute local file path in the File Viewer, including paths outside the current Project or Task Workspace. Agent File References are user-activated path-like Agent output: markdown file links, path-like inline code, and expanded tool paths. Relative locations resolve on App Server against the Task Workspace. Agent output alone never opens or reads a file; App Server mediates the user-triggered request through an opaque capability, and the Frontend does not receive raw path authority after the first open. This preserves the user's ability to inspect any file while preventing passive Agent text from becoming automatic local-file disclosure.
