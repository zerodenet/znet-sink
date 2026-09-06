import type { OverviewFeedback } from './types';

export class OverviewOperations {
  feedback = $state<OverviewFeedback>({ pending: '', target: '', message: '', error: false });
  private timer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;

  async run(target: string, operation: () => Promise<string>) {
    if (this.feedback.pending || this.disposed) return;
    clearTimeout(this.timer);
    this.feedback = { pending: target, target, message: '', error: false };
    try {
      const message = await operation();
      if (this.disposed) return;
      this.feedback = { pending: '', target, message, error: false };
      this.timer = setTimeout(() => { if (!this.disposed && this.feedback.target === target) this.feedback.message = ''; }, 3500);
    } catch (error) {
      if (!this.disposed) this.feedback = { pending: '', target, message: error instanceof Error ? error.message : String(error), error: true };
    }
  }

  destroy() { this.disposed = true; clearTimeout(this.timer); }
}
