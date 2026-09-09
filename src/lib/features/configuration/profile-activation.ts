import { EventLifecycleQueue } from '$lib/services/event-lifecycle';

export interface ActivationPorts<Profile> {
  load(id: string): Promise<Profile>;
  commit(id: string): Promise<Profile>;
  restartObservation(): Promise<void>;
  changed(): void;
  reconcile(): Promise<void>;
  warn(context: string, error: unknown): void;
}

/** Owns the complete frontend handoff, including source validation and observation.
 * The backend's configuration lock remains authoritative for all mutations. */
export class ProfileActivation<Profile> {
  private queue = new EventLifecycleQueue();
  private ports: ActivationPorts<Profile>;
  constructor(ports: ActivationPorts<Profile>) { this.ports = ports; }

  activate(id: string): Promise<Profile> {
    return this.queue.enqueue(() => this.run(id));
  }

  private async recover(context: string, action: () => Promise<unknown>) {
    try { await action(); } catch (error) { this.ports.warn(context, error); }
  }

  private async run(id: string): Promise<Profile> {
    await this.ports.load(id);
    let profile: Profile;
    try {
      profile = await this.ports.commit(id);
    } catch (error) {
      if (applicationIsUncertain(error)) {
        // The engine may have committed. Observe; do not replay apply or
        // restore a TUN layout that might now belong to the new configuration.
        await this.recover('observe uncertain profile activation', () => this.ports.restartObservation());
      }
      throw error;
    }
    // These are post-commit effects. Their failure must not imply that the
    // profile failed to commit or enter the pre-commit rollback path.
    await this.recover('restart profile observation', () => this.ports.restartObservation());
    try { this.ports.changed(); } catch (error) { this.ports.warn('publish profile change', error); }
    await this.recover('reconcile committed profile', () => this.ports.reconcile());
    return profile;
  }
}

export function applicationIsUncertain(error: unknown): boolean {
  // An unstructured invoke/channel failure cannot prove whether the backend
  // committed. Structured application rejections retain their normal rollback.
  if (!error || typeof error !== 'object') return true;
  const value = error as { code?: string; details?: {resource?: string; id?: string} };
  return !value.code || value.code === 'config_apply_uncertain'
    || (value.code === 'conflict' && value.details?.resource === 'config' && value.details?.id === 'runtime');
}
