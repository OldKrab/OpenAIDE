/**
 * Inlines a renderer bundle into its sandboxed document, so the frame needs no
 * network or package origin. The bundle travels base64-encoded because its text
 * may contain `</script>`. The Mermaid and PDF renderers share this.
 */
export function buildSelfContainedRendererDocument(
  rendererDocument: string,
  rendererScript: string,
  nonce: string,
) {
  if (!/^[a-zA-Z0-9_-]{16,128}$/.test(nonce)) throw new Error("invalid renderer nonce");
  const bytes = new TextEncoder().encode(rendererScript);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 32_768));
  }
  const payload = globalThis.btoa(binary);
  const embeddedRenderer = `<script nonce="${nonce}" type="application/octet-stream" id="openaide-renderer-payload">${payload}</script>
    <script nonce="${nonce}">
      const payload = document.getElementById("openaide-renderer-payload").textContent || "";
      const binary = atob(payload);
      const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
      const script = document.createElement("script");
      script.nonce = ${JSON.stringify(nonce)};
      script.textContent = new TextDecoder().decode(bytes);
      document.head.append(script);
    </script>`;
  const externalScript = new RegExp(
    `<script nonce="${nonce}" src="[^"]+"></script>`,
  );
  if (!externalScript.test(rendererDocument)) throw new Error("renderer unavailable");
  return rendererDocument.replace(externalScript, embeddedRenderer);
}
