import { useEffect, useRef } from "react";
import type { EditorSelectionState } from "./composerEditorSelection";

type SavedDraft = {
  caret: number;
  text: string;
};

export type ComposerHistoryNavigation = {
  caret: number;
  text: string;
};

/**
 * Owns only ephemeral browsing state; accepted history remains an App Server projection.
 */
export function useComposerHistory({
  load,
  refreshKey,
  scopeKey,
}: {
  load?: () => Promise<string[]>;
  refreshKey?: number | string;
  scopeKey?: string;
}) {
  const entriesRef = useRef<string[]>([]);
  const generationRef = useRef(0);
  const loadRef = useRef(load);
  const positionRef = useRef<number | undefined>(undefined);
  const savedDraftRef = useRef<SavedDraft | undefined>(undefined);
  loadRef.current = load;

  useEffect(() => {
    generationRef.current += 1;
    const generation = generationRef.current;
    entriesRef.current = [];
    positionRef.current = undefined;
    savedDraftRef.current = undefined;
    if (!scopeKey || !loadRef.current) return;
    void loadRef.current()
      .then((entries) => {
        if (generationRef.current !== generation) return;
        entriesRef.current = entries;
      })
      .catch((error: unknown) => {
        if (generationRef.current !== generation) return;
        console.warn("[OpenAIDE] Composer History unavailable", {
          error_kind: error instanceof Error && error.name ? error.name : typeof error,
          scope_key: scopeKey,
        });
      });
  }, [refreshKey, scopeKey]);

  return {
    edited(text: string, caret: number) {
      if (positionRef.current === undefined) return;
      positionRef.current = undefined;
      savedDraftRef.current = { caret, text };
    },
    navigate(
      direction: "newer" | "older",
      currentText: string,
      selection: EditorSelectionState,
    ): ComposerHistoryNavigation | undefined {
      if (selection.start !== selection.end) return undefined;
      const entries = entriesRef.current;
      const position = positionRef.current;
      if (position === undefined) {
        if (direction === "older" && !selection.firstVisualLine) return undefined;
        if (direction === "newer" && !selection.lastVisualLine) return undefined;
      } else if (entries[position] !== currentText || selection.end !== currentText.length) {
        // Untouched recalled entries cycle from their end; elsewhere arrows remain native.
        return undefined;
      }
      if (direction === "older") {
        if (entries.length === 0) return undefined;
        if (position === undefined) {
          savedDraftRef.current = { caret: selection.start, text: currentText };
          positionRef.current = 0;
        } else if (position < entries.length - 1) {
          positionRef.current = position + 1;
        } else {
          return undefined;
        }
        const text = entries[positionRef.current];
        return { caret: text.length, text };
      }
      if (position === undefined) return undefined;
      if (position > 0) {
        positionRef.current = position - 1;
        const text = entries[position - 1];
        return { caret: text.length, text };
      }
      positionRef.current = undefined;
      const saved = savedDraftRef.current ?? { caret: currentText.length, text: currentText };
      savedDraftRef.current = undefined;
      return saved;
    },
  };
}
