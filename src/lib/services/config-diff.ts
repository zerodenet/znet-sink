import type { EffectiveConfigSource } from '$lib/services/core';

export interface EffectiveConfigDiffEntry {
  path: string;
  source: string;
  before: unknown;
  after: unknown;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function equal(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function sourceForPath(path: string, sources: EffectiveConfigSource[]): string {
  const source = sources.find((candidate) => candidate.paths.some(
    (sourcePath) => path === sourcePath || path.startsWith(`${sourcePath}.`),
  ));
  return source?.label ?? '客户端运行时覆盖';
}

export function effectiveConfigDiff(
  base: Record<string, unknown> | undefined,
  effective: Record<string, unknown> | undefined,
  sources: EffectiveConfigSource[],
): EffectiveConfigDiffEntry[] {
  if (!base || !effective) return [];
  const result: EffectiveConfigDiffEntry[] = [];

  function visit(left: unknown, right: unknown, path: string) {
    if (equal(left, right)) return;
    if (isObject(left) && isObject(right)) {
      const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
      for (const key of [...keys].sort()) {
        visit(left[key], right[key], path ? `${path}.${key}` : key);
      }
      return;
    }
    result.push({
      path: path || '$',
      source: sourceForPath(path, sources),
      before: left,
      after: right,
    });
  }

  visit(base, effective, '');
  return result;
}

export function compactConfigValue(value: unknown): string {
  if (value === undefined) return '未定义';
  const encoded = JSON.stringify(value) ?? String(value);
  if (encoded.length <= 120) return encoded;
  return `${encoded.slice(0, 117)}…`;
}
