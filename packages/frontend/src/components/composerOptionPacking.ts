export type ComposerOptionPackingMeasurement = {
  availableWidth: number;
  gap: number;
  optionWidths: number[];
  /** Indexed by hidden option count; index zero is unused when grouping. */
  overflowWidths: number[];
};

/** Returns the largest leading option set that fits beside its overflow control. */
export function visibleComposerOptionCount({
  availableWidth,
  gap,
  optionWidths,
  overflowWidths,
}: ComposerOptionPackingMeasurement) {
  if (optionWidths.length === 0) return 0;
  if (rowWidth(optionWidths, gap) <= availableWidth) return optionWidths.length;

  for (let visibleCount = optionWidths.length - 1; visibleCount >= 0; visibleCount -= 1) {
    const hiddenCount = optionWidths.length - visibleCount;
    const visibleWidths = optionWidths.slice(0, visibleCount);
    const overflowWidth = overflowWidths[hiddenCount] ?? overflowWidths.at(-1) ?? 0;
    if (rowWidth([...visibleWidths, overflowWidth], gap) <= availableWidth) return visibleCount;
  }

  return 0;
}

function rowWidth(widths: number[], gap: number) {
  if (widths.length === 0) return 0;
  return widths.reduce((total, width) => total + width, 0) + gap * (widths.length - 1);
}
