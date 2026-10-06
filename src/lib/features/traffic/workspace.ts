import { TrafficSession, type TrafficView } from '$lib/features/traffic/session';
import type { TrafficGateway, TrafficScope } from './types';

/** The app owns sampling; mounted pages only observe the shared view. */
export class TrafficWorkspace {
  readonly session: TrafficSession;
  tab: 'endpoints' | 'traffic' = 'endpoints';
  statistics: { kind: TrafficScope['kind']; plane: string; query: string; offset: number } = {
    kind: 'global', plane: 'flow', query: '', offset: 0,
  };
  private listeners = new Set<(view: TrafficView) => void>();
  private started = false;
  private disposed = false;
  constructor(gateway: TrafficGateway) {
    this.session = new TrafficSession(gateway, view => {
      for (const listener of this.listeners) listener(view);
    });
  }
  connect(listener: (view: TrafficView) => void): () => void {
    if (this.disposed) throw new Error('统计工作区已关闭');
    this.listeners.add(listener);
    listener(this.session.view);
    if (!this.started) {
      this.started = true;
      void this.session.start();
    }
    return () => { this.listeners.delete(listener); };
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.listeners.clear();
    this.session.dispose();
  }
}
