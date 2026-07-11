# ACP Filesystem Through Host Bridge

OpenAIDE will advertise ACP filesystem capabilities only when the current Host can satisfy them through the Host file bridge. ACP `fs/read_text_file` and `fs/write_text_file` must respect execution-root boundaries, unsaved editor buffers, dirty-buffer conflict checks, encoding/line-ending behavior, and redacted error handling; the runtime must not bypass the Host and perform direct disk reads or writes for these ACP client methods.
