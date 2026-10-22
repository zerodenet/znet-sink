/** Presentation-only equality: order in an object is not an unsaved edit. */
export function settingsDraftKey(value: unknown): string {
  const ordered = (input: unknown): unknown => {
    if (Array.isArray(input)) return input.map(ordered);
    if (input && typeof input === 'object') return Object.fromEntries(
      Object.entries(input).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, ordered(item)]),
    );
    return input;
  };
  return JSON.stringify(ordered(value));
}
export interface SettingsDraftRead {
  baseline: string | null;
  draft: string;
}
export function captureSettingsDraftRead(baseline: unknown | null, draft: unknown): SettingsDraftRead {
  return { baseline: baseline === null ? null : settingsDraftKey(baseline), draft: settingsDraftKey(draft) };
}
/** A refresh may replace only an unchanged clean form. It never rebases edits. */
export function canAcceptSettingsDraftRead(read: SettingsDraftRead, baseline: unknown | null, draft: unknown): boolean {
  const currentBaseline = baseline === null ? null : settingsDraftKey(baseline);
  return read.baseline === currentBaseline && read.draft === settingsDraftKey(draft)
    && (read.baseline === null || read.draft === read.baseline);
}
