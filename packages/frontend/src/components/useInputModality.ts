import { useEffect } from "react";

/** Keeps pointer focus from looking like keyboard focus on browsers with broad :focus-visible heuristics. */
export function useInputModality() {
  useEffect(() => {
    if (typeof document === "undefined") return undefined;
    const body = document.body;
    const markPointer = () => {
      body.dataset.inputModality = "pointer";
    };
    const markKeyboard = (event: KeyboardEvent) => {
      if (event.key !== "Unidentified") body.dataset.inputModality = "keyboard";
    };
    document.addEventListener("pointerdown", markPointer, true);
    document.addEventListener("keydown", markKeyboard, true);
    return () => {
      document.removeEventListener("pointerdown", markPointer, true);
      document.removeEventListener("keydown", markKeyboard, true);
      delete body.dataset.inputModality;
    };
  }, []);
}
