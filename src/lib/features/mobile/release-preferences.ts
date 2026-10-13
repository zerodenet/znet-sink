import type { MobileReleaseGateway, ReleasePreferences, ReleasePreferencesSnapshot } from '$lib/types/mobile-release';
export interface ReleasePreferenceState {
  phase: 'loading' | 'ready' | 'saving' | 'unknown' | 'checking' | 'error';
  snapshot: ReleasePreferencesSnapshot | null;
  draft: ReleasePreferences;
  error: string | null;
  notice: string | null;
}
export class MobileReleasePreferences {
  state: ReleasePreferenceState = { phase: 'loading', snapshot: null, draft: { clientAutoCheck: false, clientChannel: 'stable', kernelChannel: 'stable' }, error: null, notice: null };
  private disposed = false;
  private generation = 0;
  constructor(private gateway: Pick<MobileReleaseGateway, 'preferences' | 'savePreferences'>, private notify: (state: ReleasePreferenceState) => void) {}
  private publish(state: ReleasePreferenceState) { if (this.disposed) return; this.state = state; this.notify(state); }
  edit(draft: ReleasePreferences) {
    if (this.state.phase !== 'ready') return;
    this.publish({ ...this.state, draft: { ...draft }, notice: null, error: null });
  }
  async read() {
    if (this.disposed || this.state.phase === 'saving' || this.state.phase === 'checking') return;
    const unknown = this.state.phase === 'unknown';
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: unknown ? 'checking' : 'loading', error: null });
    try {
      const snapshot = await this.gateway.preferences();
      if (this.disposed || generation !== this.generation) return;
      this.publish({ phase: 'ready', snapshot, draft: { ...snapshot.preferences }, error: null,
        notice: unknown ? '已读取当前保存值；原保存回执仍未确认，未重放保存。' : null });
    } catch {
      if (this.disposed || generation !== this.generation) return;
      this.publish({ ...this.state, phase: unknown ? 'unknown' : 'error', error: '发行偏好读取失败，请重试。' });
    }
  }
  async save() {
    if (this.disposed || this.state.phase !== 'ready' || !this.state.snapshot) return;
    const { expectedRevision } = this.state.snapshot;
    const draft = { ...this.state.draft };
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'saving', notice: null, error: null });
    try {
      const snapshot = await this.gateway.savePreferences(expectedRevision, draft);
      if (this.disposed || generation !== this.generation) return;
      this.publish({ phase: 'ready', snapshot, draft: { ...snapshot.preferences }, error: null,
        notice: '发行偏好已保存。移动后台自动检查尚未适配，此开关只保存意图。' });
    } catch (error) {
      if (this.disposed || generation !== this.generation) return;
      const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
      const rejected = ['conflict', 'invalid_argument', 'forbidden', 'mode_restricted'].includes(String(code));
      this.publish({ ...this.state, phase: rejected ? 'error' : 'unknown',
        error: rejected ? '保存被拒绝或设置已变化；草稿保留，请重新读取已保存值。' : '保存结果未确认；请核对当前保存值，不会自动重复提交。' });
    }
  }
  destroy() { this.disposed = true; this.generation++; }
}
