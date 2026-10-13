import type { SettingsTransferGateway, SettingsTransferPreview, SettingsTransferReceipt } from '$lib/types/settings-transfer';
export type MigrationPhase = 'editing' | 'previewing' | 'ready' | 'saving' | 'saved' | 'unknown' | 'checking' | 'error';
export interface MigrationState {
  content: string; phase: MigrationPhase; preview: SettingsTransferPreview | null;
  receipt: SettingsTransferReceipt | null; error: string | null;
}
const refused = new Set(['invalid_argument', 'conflict', 'not_found', 'io_error', 'mode_restricted', 'forbidden', 'permission_denied']);
function safeError(error: unknown): string {
  const value = error && typeof error === 'object' ? error as { code?: unknown; details?: { fieldPath?: unknown } } : {};
  if (value.code === 'conflict') return '设置、配置或实例已变化，请重新预览。';
  if (value.code === 'forbidden' || value.code === 'permission_denied' || value.code === 'mode_restricted') return '当前宿主授权或展示模式不允许设置迁移。';
  if (value.code === 'not_found') return '原操作记录已失效；请读取当前设置后重新预览。';
  if (value.code === 'io_error') return '本地存储失败，原设置保留。';
  if (value.code === 'invalid_argument') {
    if (value.details?.fieldPath === 'document.size') return '迁移材料超过 2 MiB 限制，请缩小内容。';
    const path = value.details?.fieldPath;
    return `迁移材料包含不支持、无效或敏感的字段${typeof path === 'string' && /^[a-zA-Z][a-zA-Z0-9.]*$/.test(path) ? `（${path}）` : ''}。`;
  }
  return '操作结果未确认，请重新核对；不会自动重放导入。';
}
/** UI state only; preview and publication authority stay in the native host. */
export class SettingsMigration {
  state: MigrationState = { content: '', phase: 'editing', preview: null, receipt: null, error: null };
  private generation = 0;
  private alive = true;
  constructor(private gateway: SettingsTransferGateway, private changed: (state: MigrationState) => void) {}
  private set(patch: Partial<MigrationState>) { this.state = { ...this.state, ...patch }; if (this.alive) this.changed(this.state); }
  get busy() { return ['previewing', 'saving', 'checking'].includes(this.state.phase); }
  edit(content: string) {
    if (this.busy || this.state.phase === 'unknown') return;
    this.generation++;
    this.set({ content, phase: 'editing', preview: null, receipt: null, error: null });
  }
  invalidate() {
    if (['saving','unknown','checking'].includes(this.state.phase)) {
      this.set({ error: '所属配置已变化；先核对原操作，再在当前配置重新预览。' });
      return;
    }
    this.generation++;
    this.set({ phase: 'editing', preview: null, receipt: null, error: '所属配置已变化，请重新预览。' });
  }
  async preview() {
    if (this.busy || this.state.phase === 'unknown' || !this.state.content.trim()) return;
    const content = this.state.content, generation = ++this.generation;
    if (new TextEncoder().encode(content).byteLength > 2 * 1024 * 1024) {
      this.set({ phase: 'error', preview: null, receipt: null, error: '迁移材料超过 2 MiB 限制，请缩小内容。' });
      return;
    }
    this.set({ phase: 'previewing', preview: null, receipt: null, error: null });
    try {
      const preview = await this.gateway.preview(content);
      if (!this.alive || generation !== this.generation) return;
      this.set({ phase: 'ready', preview });
    } catch (error) { if (this.alive && generation === this.generation) this.set({ phase: 'error', error: safeError(error) }); }
  }
  async confirm() {
    if (this.state.phase !== 'ready' || !this.state.preview) return;
    const preview = this.state.preview;
    this.set({ phase: 'saving', error: null });
    try {
      const receipt = await this.gateway.commit(preview.planId);
      if (this.alive) this.set({ phase: receipt.localState === 'saved' ? 'saved' : 'unknown', receipt });
    } catch (error) {
      if (!this.alive) return;
      const code = error && typeof error === 'object' ? (error as {code?:string}).code : undefined;
      this.set({ phase: code && refused.has(code) ? 'error' : 'unknown', error: safeError(error) });
    }
  }
  async check() {
    if (this.state.phase !== 'unknown' || !this.state.preview) return;
    const id = this.state.preview.planId;
    this.set({ phase: 'checking', error: null });
    try {
      const receipt = await this.gateway.status(id);
      if (this.alive) this.set({ phase: receipt.localState === 'saved' ? 'saved' : receipt.localState === 'not-committed' ? 'error' : 'unknown', receipt, error: receipt.localState === 'not-committed' ? '尚无保存回执，请核对后重新预览。' : null });
    } catch (error) { if (this.alive) this.set({ phase: 'unknown', error: safeError(error) }); }
  }
  destroy() { this.alive = false; this.generation++; }
}
