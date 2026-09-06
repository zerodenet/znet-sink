/** Variable-height window; only the viewport and a small margin reach the DOM. */
export function logWindow(ids: number[], heights: ReadonlyMap<number, number>, scrollTop: number, viewportHeight: number) {
  const offsets = [0];
  for (const id of ids) offsets.push(offsets[offsets.length - 1] + (heights.get(id) ?? 36));
  const rowAt = (position: number) => {
    let low = 0;
    let high = ids.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (offsets[mid + 1] <= position) low = mid + 1;
      else high = mid;
    }
    return low;
  };
  const start = Math.max(0, rowAt(Math.max(0, scrollTop)) - 5);
  const end = Math.min(ids.length, rowAt(Math.max(0, scrollTop) + Math.max(1, viewportHeight)) + 6);
  return { start, end, top: offsets[start], bottom: offsets[ids.length] - offsets[end], offsets };
}
