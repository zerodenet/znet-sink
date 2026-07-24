export interface DeltaBatchScheduler {
  setTimeout(callback: () => void, delayMs: number): unknown;
  clearTimeout(handle: unknown): void;
}

const defaultScheduler: DeltaBatchScheduler = {
  setTimeout(callback, delayMs) {
    return globalThis.setTimeout(callback, delayMs);
  },
  clearTimeout(handle) {
    globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>);
  },
};

/**
 * Coalesces a burst of deltas while guaranteeing a flush at a fixed interval.
 * New events never postpone an already scheduled flush.
 */
export class ConnectionDeltaBatcher<T> {
  private pending: T[] = [];
  private timer: unknown = null;
  private readonly delayMs: number;
  private readonly onFlush: (items: T[]) => void;
  private readonly scheduler: DeltaBatchScheduler;

  constructor(
    delayMs: number,
    onFlush: (items: T[]) => void,
    scheduler: DeltaBatchScheduler = defaultScheduler,
  ) {
    this.delayMs = delayMs;
    this.onFlush = onFlush;
    this.scheduler = scheduler;
  }

  push(items: T[]) {
    if (items.length === 0) return;
    this.pending.push(...items);
    if (this.timer !== null) return;

    this.timer = this.scheduler.setTimeout(() => {
      this.timer = null;
      this.flush();
    }, this.delayMs);
  }

  flush() {
    if (this.timer !== null) {
      this.scheduler.clearTimeout(this.timer);
      this.timer = null;
    }
    if (this.pending.length === 0) return;

    const items = this.pending;
    this.pending = [];
    this.onFlush(items);
  }

  destroy() {
    if (this.timer !== null) {
      this.scheduler.clearTimeout(this.timer);
      this.timer = null;
    }
    this.pending = [];
  }
}
