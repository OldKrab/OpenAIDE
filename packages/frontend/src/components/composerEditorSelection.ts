export type EditorSelection = {
  start: number;
  end: number;
};

/** Capture selection in the composer's plain-text coordinate system, where each BR is one newline. */
export function captureFocusedEditorSelection(root: HTMLElement | null): EditorSelection | undefined {
  if (!root || root.ownerDocument.activeElement !== root) return undefined;
  return selectionOffsets(root);
}

/** Restore a plain-text selection without losing offsets to rendered BR elements. */
export function restoreEditorSelection(root: HTMLElement, selection: EditorSelection) {
  if (root.ownerDocument.activeElement !== root) return;
  setSelectionOffsets(root, selection.start, selection.end);
}

export function selectionOffsets(root: HTMLElement) {
  const fallback = editableText(root).length;
  const selection = root.ownerDocument?.getSelection?.();
  if (!selection || selection.rangeCount === 0) return { start: fallback, end: fallback };
  const range = selection.getRangeAt(0);
  return {
    start: boundaryOffset(root, range.startContainer, range.startOffset),
    end: boundaryOffset(root, range.endContainer, range.endOffset),
  };
}

export function setSelectionOffsets(root: HTMLElement, start: number, end: number) {
  const doc = root.ownerDocument;
  const selection = doc.getSelection();
  if (!selection) return;
  const startBoundary = textBoundary(root, start);
  const endBoundary = textBoundary(root, end);
  if (!startBoundary || !endBoundary) return;
  const range = doc.createRange();
  range.setStart(startBoundary.node, startBoundary.offset);
  range.setEnd(endBoundary.node, endBoundary.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

function editableText(root: HTMLElement) {
  const text = root.innerText ?? root.textContent ?? "";
  return text.replace(/\n$/, "");
}

function boundaryOffset(root: HTMLElement, container: Node, offset: number) {
  if (typeof root.contains === "function" && container !== root && !root.contains(container)) {
    return editableText(root).length;
  }
  const boundary = nodeBoundaryOffset(root, container, offset, 0);
  return boundary ?? editableText(root).length;
}

function textBoundary(root: HTMLElement, offset: number) {
  return boundaryAtTextOffset(root, Math.max(0, offset))
    ?? { node: root, offset: root.childNodes.length };
}

function nodeBoundaryOffset(node: Node, target: Node, targetOffset: number, consumed: number): number | undefined {
  if (node === target) {
    if (isTextNode(node)) return consumed + Math.min(targetOffset, node.textContent?.length ?? 0);
    let offset = consumed;
    for (let index = 0; index < Math.min(targetOffset, node.childNodes.length); index += 1) {
      offset += editorNodeLength(node.childNodes[index]);
    }
    return offset;
  }
  if (isTextNode(node) || isBreakNode(node)) return undefined;

  let offset = consumed;
  for (const child of Array.from(node.childNodes)) {
    const result = nodeBoundaryOffset(child, target, targetOffset, offset);
    if (result !== undefined) return result;
    offset += editorNodeLength(child);
  }
  return undefined;
}

function boundaryAtTextOffset(parent: Node, requestedOffset: number): { node: Node; offset: number } | undefined {
  let remaining = requestedOffset;
  for (let index = 0; index < parent.childNodes.length; index += 1) {
    const child = parent.childNodes[index];
    const length = editorNodeLength(child);
    if (remaining <= length) {
      if (isTextNode(child)) return { node: child, offset: Math.min(remaining, length) };
      if (isBreakNode(child)) return { node: parent, offset: index + (remaining > 0 ? 1 : 0) };
      return boundaryAtTextOffset(child, remaining) ?? { node: parent, offset: index };
    }
    remaining -= length;
  }
  return remaining === 0 ? { node: parent, offset: parent.childNodes.length } : undefined;
}

function editorNodeLength(node: Node): number {
  if (isTextNode(node)) return node.textContent?.length ?? 0;
  if (isBreakNode(node)) return 1;
  return Array.from(node.childNodes).reduce((length, child) => length + editorNodeLength(child), 0);
}

function isTextNode(node: Node) {
  return node.nodeType === 3 || node.childNodes === undefined;
}

function isBreakNode(node: Node) {
  return node.nodeType === 1 && (node as Element).tagName === "BR";
}
