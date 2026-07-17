import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { visibleComposerOptionCount } from "./composerOptionPacking";

type MeasureNode = Pick<HTMLElement, "getBoundingClientRect">;

/** Measures live control labels so the composer can group only the suffix that does not fit. */
export function useComposerOptionPacking(optionCount: number, measurementKey: string) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const measurementSurfaceRef = useRef<HTMLDivElement | null>(null);
  const optionMeasureRefs = useRef<Array<MeasureNode | null>>([]);
  const overflowMeasureRefs = useRef<Array<MeasureNode | null>>([]);
  const measurementAvailable = typeof globalThis.ResizeObserver === "function";
  const [measuredVisibleCount, setMeasuredVisibleCount] = useState(
    measurementAvailable ? 0 : optionCount,
  );

  const setOptionMeasureRef = useCallback((index: number, node: HTMLDivElement | null) => {
    optionMeasureRefs.current[index] = node;
  }, []);
  const setOverflowMeasureRef = useCallback((hiddenCount: number, node: HTMLDivElement | null) => {
    overflowMeasureRefs.current[hiddenCount] = node;
  }, []);

  useLayoutEffect(() => {
    if (!measurementAvailable) {
      setMeasuredVisibleCount(optionCount);
      return undefined;
    }

    const measure = () => {
      const container = containerRef.current;
      const optionNodes = optionMeasureRefs.current.slice(0, optionCount);
      if (!container || optionNodes.some((node) => !node)) return;

      const availableWidth = container.getBoundingClientRect().width;
      const optionWidths = optionNodes.map((node) => node?.getBoundingClientRect().width ?? 0);
      const overflowWidths = Array.from({ length: optionCount + 1 }, (_, hiddenCount) =>
        overflowMeasureRefs.current[hiddenCount]?.getBoundingClientRect().width ?? 0);
      const computedGap = typeof globalThis.getComputedStyle === "function"
        ? Number.parseFloat(globalThis.getComputedStyle(container).columnGap)
        : Number.NaN;

      setMeasuredVisibleCount(visibleComposerOptionCount({
        availableWidth,
        gap: Number.isFinite(computedGap) ? computedGap : 4,
        optionWidths,
        overflowWidths,
      }));
    };

    measure();
    const observer = new ResizeObserver(measure);
    if (containerRef.current) observer.observe(containerRef.current);
    if (measurementSurfaceRef.current) observer.observe(measurementSurfaceRef.current);
    return () => observer.disconnect();
  }, [measurementAvailable, measurementKey, optionCount]);

  return {
    containerRef,
    measurementAvailable,
    measurementSurfaceRef,
    setOptionMeasureRef,
    setOverflowMeasureRef,
    visibleCount: Math.min(measuredVisibleCount, optionCount),
  };
}
