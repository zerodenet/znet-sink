import { decodeInspection, type Inspection, type InspectionKind } from '$lib/features/storage-inspection/model';
export interface InspectionView { busy: boolean; result: Inspection|null; historical: boolean; error: string|null }
/** UI read lifetime only, independent of all operation/close/recovery stores. */
export class InspectionSession {
  view: InspectionView = {busy:false,result:null,historical:false,error:null};
  private epoch = 0;
  private alive = true;
  private kind: InspectionKind;
  private read: () => Promise<unknown>;
  private publish: (view: InspectionView) => void;
  constructor(kind: InspectionKind, read: () => Promise<unknown>, publish: (view: InspectionView) => void) {
    this.kind = kind; this.read = read; this.publish = publish;
  }
  private emit() { if (this.alive) this.publish({...this.view}); }
  invalidate() { this.epoch++; this.view.historical = this.view.result !== null; this.emit(); }
  async inspect() {
    if (!this.alive || this.view.busy) return;
    const epoch = this.epoch;
    this.view.busy = true; this.view.error = null; this.emit();
    try {
      const raw = await this.read();
      if (!this.alive || epoch !== this.epoch) return;
      const result = decodeInspection(this.kind, raw);
      this.view.result = result;
      this.view.historical = !result.evidence.snapshotStable && result.state !== 'not-initialized';
    } catch {
      if (!this.alive || epoch !== this.epoch) return;
      this.view.error = '只读检查未完成或返回资料无法校验；原材料与操作屏障保留，可手动重试。';
      this.view.historical = this.view.result !== null;
    } finally { this.view.busy = false; this.emit(); }
  }
  dispose() { this.alive = false; this.epoch++; }
}
