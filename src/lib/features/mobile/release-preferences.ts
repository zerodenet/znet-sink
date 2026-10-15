import type { MobileReleaseGateway, ReleasePreferenceOperation, ReleasePreferences, ReleasePreferencesSnapshot } from '$lib/types/mobile-release';
export interface ReleasePreferenceState {
  phase: 'loading' | 'ready' | 'saving' | 'confirming' | 'unknown' | 'checking' | 'error';
  snapshot: ReleasePreferencesSnapshot | null;
  currentFacts: ReleasePreferencesSnapshot | null;
  operation: ReleasePreferenceOperation | null;
  confirmAttempted: boolean;
  draft: ReleasePreferences;
  error: string | null;
  notice: string | null;
}
export const equalPreferences = (a: ReleasePreferences, b: ReleasePreferences) => a.clientAutoCheck === b.clientAutoCheck && a.clientChannel === b.clientChannel && a.kernelChannel === b.kernelChannel;
const u64 = (v: unknown): v is string => typeof v === 'string' && v.length <= 20 && /^(0|[1-9]\d*)$/.test(v) && BigInt(v) <= 18446744073709551615n;
const channel = (v: unknown) => v === 'stable' || v === 'preview' || v === 'test';
function validPreferences(p: ReleasePreferences) { return !!p && typeof p.clientAutoCheck === 'boolean' && channel(p.clientChannel) && channel(p.kernelChannel); }
function validSnapshot(s: ReleasePreferencesSnapshot) { return !!s && validPreferences(s.preferences) && typeof s.expectedRevision === 'string' && s.expectedRevision.length > 0 && ['native-foreground', 'not-native-host', 'storage-error', 'preferences-unconfirmed'].includes(s.schedulerState) && (s.operation === null || (!!s.operation && validOperation(s.operation))); }
function validOperation(o: ReleasePreferenceOperation, expected?: ReleasePreferenceOperation) {
  if (!o || !/^[0-9a-f]{32}$/.test(o.operationId) || typeof o.expectedRevision !== 'string' || !o.expectedRevision || !validPreferences(o.preferences)
    || (o.reviewBinding !== null && (typeof o.reviewBinding !== 'string' || !o.reviewBinding || o.reviewBinding.length > 256)) || !['prepared', 'unknown', 'saved-awaiting-confirm', 'confirmed', 'rejected'].includes(o.state) || typeof o.acknowledged !== 'boolean') return false;
  if (o.acknowledged !== (o.state === 'confirmed')) return false;
  if (expected && (o.operationId !== expected.operationId || o.expectedRevision !== expected.expectedRevision || !equalPreferences(o.preferences, expected.preferences))) return false;
  if (o.state === 'saved-awaiting-confirm' || o.state === 'confirmed') {
    const r = o.receipt;
    if (!r || r.ownerOperation !== o.operationId || !u64(r.beforeEpoch) || !u64(r.afterEpoch) || BigInt(r.afterEpoch) !== BigInt(r.beforeEpoch) + 1n) return false;
  }
  return o.state !== 'confirmed' || o.acknowledged;
}
function operationId() { return crypto.randomUUID().replaceAll('-', ''); }
export class MobileReleasePreferences {
  state: ReleasePreferenceState = { phase: 'loading', snapshot: null, currentFacts: null, operation: null, confirmAttempted: false,
    draft: { clientAutoCheck: false, clientChannel: 'stable', kernelChannel: 'stable' }, error: null, notice: null };
  private disposed = false;
  private generation = 0;
  constructor(private gateway: Pick<MobileReleaseGateway, 'preferences' | 'savePreferences' | 'preferenceOperation' | 'confirmPreferences'>,
    private notify: (state: ReleasePreferenceState) => void, private newOperationId = operationId) {}
  private publish(state: ReleasePreferenceState) { if (this.disposed) return; this.state = state; this.notify(state); }
  get dirty() { return !!this.state.snapshot && !equalPreferences(this.state.draft, this.state.snapshot.preferences); }
  edit(draft: ReleasePreferences) {
    if (this.state.phase !== 'ready' || !validPreferences(draft)) return;
    this.publish({ ...this.state, draft: { ...draft }, notice: null, error: null });
  }
  discardDraft() {
    if (this.disposed || !['ready', 'loading'].includes(this.state.phase) || !this.state.snapshot) return;
    // A concurrent read may report facts, but cannot revive an explicitly discarded draft.
    this.generation++;
    this.publish({ ...this.state, phase: 'ready', draft: { ...this.state.snapshot.preferences }, notice: null, error: null });
  }
  async discardAndReload() {
    // This is an explicit UI discard, never a way around an unresolved write.
    if (this.disposed || this.state.phase !== 'ready' || !this.state.snapshot) return;
    const baseline = { ...this.state.snapshot.preferences };
    this.publish({ ...this.state, phase: 'loading', snapshot: null, currentFacts: null, operation: null,
      draft: baseline, error: null, notice: '已明确放弃草稿，正在重新读取发行偏好；未提交保存或确认。' });
    await this.read();
  }
  async read() {
    if (this.disposed || ['saving', 'confirming', 'checking', 'unknown'].includes(this.state.phase)) return;
    const dirty = this.dirty;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'loading', error: null });
    try {
      const snapshot = await this.gateway.preferences();
      if (this.disposed || generation !== this.generation) return;
      if (!validSnapshot(snapshot)) throw new Error('invalid snapshot');
      const op = snapshot.operation;
      if (op && !validOperation(op)) throw new Error('invalid operation');
      if (op && !op.acknowledged && op.state !== 'rejected') {
        this.publish({ ...this.state, phase: 'unknown', currentFacts: snapshot, operation: op, draft: { ...op.preferences },
          notice: '已找到原偏好操作；当前值不是保存回执，请核对原操作。', error: '原偏好保存尚未确认，不会重新提交。' });
      } else if (dirty && this.state.snapshot) {
        this.publish({ ...this.state, phase: 'ready', currentFacts: snapshot, notice: '已读取当前保存值；未覆盖草稿或更换原版本前提。' });
      } else {
        this.publish({ ...this.state, phase: 'ready', snapshot, currentFacts: snapshot, draft: { ...snapshot.preferences }, operation: null,
          confirmAttempted: false, notice: null, error: null });
      }
    } catch {
      if (this.disposed || generation !== this.generation) return;
      this.publish({ ...this.state, phase: this.state.snapshot ? 'ready' : 'error', error: '发行偏好读取失败，请重试。' });
    }
  }
  async save() {
    if (this.disposed || this.state.phase !== 'ready' || !this.state.snapshot || !this.dirty) return;
    const original: ReleasePreferenceOperation = { operationId: this.newOperationId(), expectedRevision: this.state.snapshot.expectedRevision,
      preferences: { ...this.state.draft }, state: 'prepared', acknowledged: false, reviewBinding: null, reason: null, receipt: null };
    if (!validOperation(original)) { this.publish({ ...this.state, error: '无法安全创建原操作编号，草稿保留。' }); return; }
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'saving', operation: original, confirmAttempted: false, notice: null, error: null });
    try {
      const snapshot = await this.gateway.savePreferences(original.expectedRevision, original.preferences, original.operationId);
      if (this.disposed || generation !== this.generation) return;
      if (!validSnapshot(snapshot) || !snapshot.operation || !validOperation(snapshot.operation, original)
        || snapshot.operation.state !== 'saved-awaiting-confirm' || !equalPreferences(snapshot.preferences, original.preferences)) throw new Error('invalid receipt');
      this.publish({ ...this.state, phase: 'unknown', currentFacts: snapshot, operation: snapshot.operation, notice: '原操作已有本地持久回执；只有原审阅身份有效时才能确认，未重新写入偏好。' });
      await this.confirm(); // Original explicit save authorizes this one acknowledgement, never a retry.
    } catch (error) {
      if (this.disposed || generation !== this.generation) return;
      const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
      const rejected = ['conflict', 'invalid_argument', 'forbidden', 'mode_restricted', 'release_preferences_rejected'].includes(String(code));
      this.publish({ ...this.state, phase: rejected ? 'ready' : 'unknown', operation: rejected ? null : this.state.operation,
        error: rejected ? '保存被明确拒绝；草稿与原版本保留，可修正后提交。' : '保存结果未确认；只能核对原操作，不会自动重复提交。' });
    }
  }
  async checkOperation() {
    if (this.disposed || this.state.phase !== 'unknown' || !this.state.operation) return;
    const original = this.state.operation;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'checking', error: null });
    try {
      const op = await this.gateway.preferenceOperation(original.operationId);
      if (this.disposed || generation !== this.generation) return;
      if (!op || !validOperation(op, original)) throw new Error('invalid operation');
      if (op.state === 'confirmed' && op.acknowledged) {
        // Only an original owned receipt can clear unknown; a matching current value cannot.
        const snapshot = await this.gateway.preferences();
        if (this.disposed || generation !== this.generation) return;
        if (!validSnapshot(snapshot) || !snapshot.operation || !validOperation(snapshot.operation, original)
          || snapshot.operation.state !== 'confirmed' || !snapshot.operation.acknowledged || !equalPreferences(snapshot.preferences, original.preferences)) throw new Error('invalid acknowledgement');
        this.acceptConfirmed(snapshot);
      } else if (op.state === 'rejected') {
        this.publish({ ...this.state, phase: 'ready', operation: null, error: '原保存已明确拒绝；草稿与原版本保留。', notice: null });
      } else {
        this.publish({ ...this.state, phase: 'unknown', operation: op, error: null,
          notice: op.state === 'saved-awaiting-confirm' ? '原操作已有本地持久回执；请审阅后明确确认。此确认不再次写入偏好。' : '原操作仍未确认；当前值不能代替原回执，未重放保存。' });
      }
    } catch {
      if (this.disposed || generation !== this.generation) return;
      this.publish({ ...this.state, phase: 'unknown', error: '原操作核对失败；草稿和未知屏障保留，请稍后只读核对。' });
    }
  }
  private acceptConfirmed(snapshot: ReleasePreferencesSnapshot) {
    this.publish({ ...this.state, phase: 'ready', snapshot, currentFacts: snapshot, draft: { ...snapshot.preferences }, operation: snapshot.operation,
      error: null, notice: '发行偏好已保存并确认。前台到期检查由宿主执行，系统后台调度仍未适配。' });
  }
  async confirm() {
    const original = this.state.operation;
    if (this.disposed || this.state.phase !== 'unknown' || this.state.confirmAttempted || !original
      || original.state !== 'saved-awaiting-confirm' || !original.reviewBinding || !validOperation(original)) return;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'confirming', confirmAttempted: true, error: null });
    try {
      const snapshot = await this.gateway.confirmPreferences(original.operationId, original.reviewBinding);
      if (this.disposed || generation !== this.generation) return;
      if (!validSnapshot(snapshot) || !snapshot.operation || !validOperation(snapshot.operation, original)
        || snapshot.operation.state !== 'confirmed' || !snapshot.operation.acknowledged || !equalPreferences(snapshot.preferences, original.preferences)) throw new Error('invalid acknowledgement');
      this.acceptConfirmed(snapshot);
    } catch (error) {
      if (this.disposed || generation !== this.generation) return;
      const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : null;
      const preflightRejected = code !== null && ['release_check_invalidated', 'invalid_argument', 'forbidden', 'mode_restricted'].includes(code);
      this.publish({ ...this.state, phase: 'unknown',
        operation: preflightRejected ? { ...original, reviewBinding: null } : this.state.operation,
        confirmAttempted: !preflightRejected,
        error: preflightRejected ? '原审阅身份已被明确拒绝，未确认回执；请先只读核对原操作，取得新审阅后再明确确认。不会重新保存。'
          : '原回执确认结果未确认；只读核对原操作，不重复确认或保存。' });
    }
  }
  destroy() { this.disposed = true; this.generation++; }
}
