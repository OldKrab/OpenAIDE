export function androidSystemAppearance(host: Window, send: (color: string) => void) {
  const body = host.document?.body;
  if (!body) return { refresh() {}, dispose() {} };
  const canvas = host.document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  let previous: string | undefined;
  const update = (force = false) => {
    if (!context) return;
    context.clearRect(0, 0, 1, 1);
    const style = host.getComputedStyle(body);
    context.fillStyle = style.getPropertyValue("--oa-panel").trim() || style.backgroundColor;
    context.fillRect(0, 0, 1, 1);
    const channels = context.getImageData(0, 0, 1, 1).data;
    if (channels[3] !== 255) return;
    const color = `#${Array.from(channels.slice(0, 3), channel => channel.toString(16).padStart(2, "0")).join("")}`;
    if (force || previous !== color) {
      previous = color;
      send(color);
    }
  };
  const observer = new MutationObserver(() => update());
  observer.observe(body, { attributes: true, attributeFilter: ["data-theme"] });
  return {
    refresh() {
      observer.observe(body, { attributes: true, attributeFilter: ["data-theme"] });
      update(true);
    },
    dispose: () => observer.disconnect(),
  };
}
