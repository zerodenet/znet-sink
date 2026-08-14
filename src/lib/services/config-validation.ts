export interface ConfigValidationIssue {
  fieldPath?: string;
  message: string;
}

export interface NormalizedConfigValidationResult {
  valid: boolean;
  errors: ConfigValidationIssue[];
}

type UnknownRecord = Record<string, unknown>;

function asRecord(value: unknown): UnknownRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as UnknownRecord
    : null;
}

function asNonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined;
}

function safeJson(value: unknown): string | undefined {
  try {
    const encoded = JSON.stringify(value);
    return encoded && encoded !== '{}' ? encoded : undefined;
  } catch {
    return undefined;
  }
}

function fieldPathFromRecord(record: UnknownRecord): string | undefined {
  return asNonEmptyString(record['field_path'])
    ?? asNonEmptyString(record['fieldPath'])
    ?? asNonEmptyString(record['path']);
}

function messageFromValue(value: unknown): string | undefined {
  if (typeof value === 'string') return value.trim() || undefined;
  if (value instanceof Error) return value.message || undefined;

  const record = asRecord(value);
  if (!record) return safeJson(value);

  const direct = asNonEmptyString(record['message'])
    ?? asNonEmptyString(record['reason'])
    ?? asNonEmptyString(record['cause']);
  if (direct) return direct;

  const nestedMessage = record['message'];
  if (nestedMessage !== undefined && nestedMessage !== value) {
    const normalized = messageFromValue(nestedMessage);
    if (normalized) return normalized;
  }

  const nestedError = record['error'];
  if (nestedError !== undefined && nestedError !== value) {
    const normalized = messageFromValue(nestedError);
    if (normalized) return normalized;
  }

  return safeJson(value);
}

function issuesFromUnknown(value: unknown, depth = 0): ConfigValidationIssue[] {
  if (depth > 6 || value === null || value === undefined) return [];

  if (Array.isArray(value)) {
    return value.flatMap((item) => issuesFromUnknown(item, depth + 1));
  }

  if (typeof value === 'string') {
    return value.trim() ? [{ message: value }] : [];
  }

  const record = asRecord(value);
  if (!record) {
    const message = messageFromValue(value);
    return message ? [{ message }] : [];
  }

  // Prefer field-level diagnostics over a generic envelope/AppError message.
  for (const key of ['errors', 'issues', 'details']) {
    const nested = record[key];
    if (Array.isArray(nested) && nested.length > 0) {
      const issues = issuesFromUnknown(nested, depth + 1);
      if (issues.length > 0) return issues;
    }
  }

  // Tauri AppError.details preserves the original Zero response envelope,
  // whose `error.details[]` contains the useful config field diagnostics.
  for (const key of ['error', 'details']) {
    const nested = record[key];
    if (nested && typeof nested === 'object' && nested !== value) {
      const issues = issuesFromUnknown(nested, depth + 1);
      if (issues.length > 0) return issues;
    }
  }

  const message = messageFromValue(record['message'])
    ?? messageFromValue(record['reason'])
    ?? messageFromValue(record['cause']);
  if (!message) return [];

  return [{
    fieldPath: fieldPathFromRecord(record),
    message,
  }];
}

function failureFromRecord(record: UnknownRecord): NormalizedConfigValidationResult {
  const errors = issuesFromUnknown(record['error'])
    .concat(issuesFromUnknown(record['errors']))
    .concat(issuesFromUnknown(record['details']));

  if (errors.length > 0) {
    return { valid: false, errors };
  }

  const message = messageFromValue(record)
    ?? '内核返回了未包含错误详情的校验失败响应';
  return { valid: false, errors: [{ message }] };
}

/**
 * Normalize the GUI/kernel config validation contract without leaking IPC
 * envelope details into the editor. Supported success shapes include both
 * the current Zero command response (`accepted -> result.valid`) and an
 * already-unwrapped `{ valid }` result.
 */
export function normalizeConfigValidationResponse(
  response: unknown,
): NormalizedConfigValidationResult {
  let current: unknown = response;

  for (let depth = 0; depth < 6; depth += 1) {
    const record = asRecord(current);
    if (!record) break;

    if (record['ok'] === false || record['accepted'] === false) {
      return failureFromRecord(record);
    }

    if (record['valid'] === true) {
      return { valid: true, errors: [] };
    }

    if (record['valid'] === false) {
      const errors = issuesFromUnknown(record['errors'])
        .concat(issuesFromUnknown(record['details']))
        .concat(issuesFromUnknown(record['error']));
      return {
        valid: false,
        errors: errors.length > 0
          ? errors
          : [{ message: '内核判定配置无效，但未返回具体错误' }],
      };
    }

    if ('result' in record) {
      current = record['result'];
      continue;
    }

    break;
  }

  return {
    valid: false,
    errors: [{
      message: `无法识别内核校验响应${safeJson(response) ? `：${safeJson(response)}` : ''}`,
    }],
  };
}

/** Extract field-level Zero validation diagnostics from a rejected Tauri invoke. */
export function normalizeConfigValidationError(error: unknown): ConfigValidationIssue[] {
  const issues = issuesFromUnknown(error);
  if (issues.length > 0) return issues;

  return [{ message: getConfigEditorErrorMessage(error, '内核校验失败') }];
}

/** Never stringify an object through String(value), which yields `[object Object]`. */
export function getConfigEditorErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string') return error.trim() || fallback;
  if (error instanceof Error) return error.message || fallback;

  const message = messageFromValue(error);
  return message || fallback;
}
