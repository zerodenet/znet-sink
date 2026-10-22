import { removalConfirmed, removalInput } from '$lib/features/profile-removal/types';
import type { RemovalAction, RemovalGateway, RemovalPreview, RemovalResult } from '$lib/features/profile-removal/types';
export interface RemovalView { targetId: string | null; preview: RemovalPreview | null; loading: boolean; busy: boolean; error: unknown; result: RemovalResult | null; unconfirmed: boolean }
export class RemovalSession {
  view: RemovalView = { targetId: null, preview: null, loading: false, busy: false, error: null, result: null, unconfirmed: false };
  private epoch = 0;
  private gateway: RemovalGateway;
  private changed: (view: RemovalView) => void;
  private removed: (result: RemovalResult) => void;
  constructor(gateway: RemovalGateway, changed: (view: RemovalView) => void, removed: (result: RemovalResult) => void) { this.gateway = gateway; this.changed = changed; this.removed = removed; }
  private publish(patch: Partial<RemovalView>) { this.view = { ...this.view, ...patch }; this.changed(this.view); }
  close() { this.epoch++; this.publish({ targetId: null, preview: null, loading: false, busy: false, error: null, result: null, unconfirmed: false }); }
  async open(id: string) {
    const epoch = ++this.epoch;
    this.publish({ targetId: id, preview: null, loading: true, busy: false, error: null, result: null, unconfirmed: false });
    try {
      const preview = await this.gateway.preview(id);
      if (epoch !== this.epoch) return;
      if (preview.targetId !== id) throw new Error('删除预检目标不匹配，请重新读取');
      this.publish({ preview, unconfirmed: preview.pending });
    } catch (error) { if (epoch === this.epoch) this.publish({ error }); }
    finally { if (epoch === this.epoch) this.publish({ loading: false }); }
  }
  async commit(action: RemovalAction) {
    if (!this.view.preview || this.view.busy || this.view.loading || this.view.unconfirmed) return;
    const input = removalInput(this.view.preview, action);
    if (!input) { this.publish({ error: { message: '请确认删除范围并选择有效替代或停止；来源或状态不允许删除' } }); return; }
    const epoch = ++this.epoch;
    this.publish({ busy: true, error: null, result: null });
    try {
      const result = await this.gateway.commit(input);
      if (epoch !== this.epoch) return;
      if (removalConfirmed(input, result)) { this.publish({ result }); this.removed(result); }
      else this.publish({ result: { ...result, status: 'unconfirmed', message: result.message || '删除结果未确认，请核对原操作' }, unconfirmed: true });
    } catch (error) {
      if (epoch !== this.epoch) return;
      const code = (error as { code?: string })?.code;
      const refusal = ['invalid_argument', 'not_found', 'conflict', 'permission_denied', 'mode_restricted', 'forbidden', 'unsupported'].includes(code ?? '');
      this.publish({ error, unconfirmed: !refusal, preview: code === 'conflict' ? null : this.view.preview });
    } finally { if (epoch === this.epoch) this.publish({ busy: false }); }
  }
  async reconcile() {
    const id = this.view.targetId;
    if (!id || this.view.busy) return;
    const epoch = ++this.epoch;
    this.publish({ busy: true, error: null });
    try {
      const result = await this.gateway.reconcile(id);
      if (epoch !== this.epoch) return;
      if (result.status === 'confirmed' && result.removedId === id) { this.publish({ result, unconfirmed: false }); this.removed(result); }
      else if (result.status === 'not_deleted') { await this.open(id); if (this.view.targetId === id) this.publish({ result }); }
      else this.publish({ result, unconfirmed: true });
    } catch (error) { if (epoch === this.epoch) this.publish({ error, unconfirmed: true }); }
    finally { if (epoch === this.epoch) this.publish({ busy: false }); }
  }
}
