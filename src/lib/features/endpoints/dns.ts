import type { UserEndpoint } from './library';

/** One explicit default DNS source; unrelated endpoint definitions are retained. */
export function chooseImportedDns(rows: UserEndpoint[]): UserEndpoint[] {
  let selected = false;
  return rows.map(row => {
    if (!row.dns) return row;
    const use = !selected && row.dns.use_as_default;
    selected ||= use;
    return { ...row, dns: { ...row.dns, use_as_default: use } };
  });
}
export function mergeImportedEndpoints(existing: UserEndpoint[], incoming: UserEndpoint[]): UserEndpoint[] {
  const selected = incoming.find(row => row.dns?.use_as_default)?.tag;
  return [...existing.map(row => selected && row.tag !== selected && row.dns?.use_as_default
    ? { ...row, dns: { ...row.dns, use_as_default: false } } : row), ...incoming];
}
