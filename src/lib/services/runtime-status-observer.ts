/** Keep observations current even when a healthy kernel emits no state changes. */
export class RuntimeStatusObserver {
  private timer: ReturnType<typeof setInterval> | null = null;
  private pending: Promise<void> | null = null;
  private active = false;

  constructor(private readonly refresh: () => Promise<void>, private readonly intervalMs = 5000) {}

  private visible() {
    return typeof document === 'undefined' || document.visibilityState !== 'hidden';
  }

  private refreshIfVisible = () => {
    if (!this.active || !this.visible() || this.pending) return;
    const pending = this.refresh().catch(() => {}).finally(() => {
      if (this.pending === pending) this.pending = null;
    });
    this.pending = pending;
  };

  start() {
    if (this.active) return;
    this.active = true;
    this.timer = setInterval(this.refreshIfVisible, this.intervalMs);
    if (typeof document !== 'undefined') document.addEventListener('visibilitychange', this.refreshIfVisible);
    if (typeof window !== 'undefined') window.addEventListener('focus', this.refreshIfVisible);
  }

  stop() {
    this.active = false;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', this.refreshIfVisible);
    if (typeof window !== 'undefined') window.removeEventListener('focus', this.refreshIfVisible);
  }
}
