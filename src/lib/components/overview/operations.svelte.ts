import * as toast from '$lib/services/toast.svelte';
import { describeUiError } from '$lib/services/ui-error';
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
      toast.success(message);
      if (this.disposed) return;
      this.feedback = { pending: '', target, message, error: false };
      this.timer = setTimeout(() => { if (!this.disposed && this.feedback.target === target) this.feedback.message = ''; }, 3500);
    } catch (error) {
      const message = describeUiError(error, '操作失败，请查看应用日志').message;
      toast.error(message);
      if (!this.disposed) this.feedback = { pending: '', target, message, error: true };
    }
  }

  destroy() { this.disposed = true; clearTimeout(this.timer); }
}
