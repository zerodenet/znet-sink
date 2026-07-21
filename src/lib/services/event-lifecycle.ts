/** Serialize event-stream lifecycle operations so a late stop cannot overtake
 * a start and invalidate the backend generation that the frontend retained. */
export class EventLifecycleQueue {
  private tail: Promise<void> = Promise.resolve();

  enqueue(operation: () => Promise<void>): Promise<void> {
    const next = this.tail.then(operation, operation);
    this.tail = next.catch(() => {});
    return next;
  }
}
