import { useCallback, useRef } from "react";

/** Keeps a callback interface stable while routing calls to the latest controller closure. */
export function useCurrentCallback<Arguments extends unknown[], Result>(
  callback: (...args: Arguments) => Result,
) {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;
  return useCallback((...args: Arguments) => callbackRef.current(...args), []);
}
