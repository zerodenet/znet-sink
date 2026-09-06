/** WebView errors can come from another realm and fail instanceof Error. */
export function describeUiError(error: unknown, fallback = 'Unhandled frontend error') {
  const value = error !== null && typeof error === 'object'
    ? error as { message?: unknown; name?: unknown; stack?: unknown } : null;
  return {
    message: typeof value?.message === 'string' && value.message ? value.message
      : typeof error === 'string' && error ? error : fallback,
    name: typeof value?.name === 'string' ? value.name : undefined,
    stack: typeof value?.stack === 'string' ? value.stack : undefined,
  };
}
