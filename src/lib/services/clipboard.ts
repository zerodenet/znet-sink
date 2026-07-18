/** Copy text in both Tauri WebView and browser development environments. */
export async function copyTextToClipboard(text: string): Promise<void> {
  let clipboardError: unknown;

  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch (error) {
    clipboardError = error;
  }

  if (typeof document === 'undefined') {
    throw clipboardError instanceof Error
      ? clipboardError
      : new Error('当前环境不支持剪贴板');
  }

  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.setAttribute('readonly', '');
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  textarea.style.pointerEvents = 'none';
  document.body.appendChild(textarea);
  textarea.select();

  try {
    if (!document.execCommand('copy')) {
      throw clipboardError instanceof Error
        ? clipboardError
        : new Error('系统拒绝了剪贴板写入');
    }
  } finally {
    textarea.remove();
  }
}
