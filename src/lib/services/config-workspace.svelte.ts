import { applyConfigWorkspace, getConfigWorkspaceSnapshot, planConfigWorkspace, saveConfigWorkspaceLocal, formatConfigWorkspace, validateConfigWorkspace, inspectConfigWorkspaceOperation, discardConfigWorkspacePlan, reviewConfigWorkspaceRecovery, completeConfigWorkspaceLocal } from '$lib/services/core';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
import { normalizeConfigValidationError } from '$lib/services/config-validation';
import { success, warning } from '$lib/services/toast.svelte';
import type { ConfigCompositionLayer, ConfigImpactItem, ConfigPlanApplyResult, ConfigTransactionReceipt, ConfigWorkspacePlan, ConfigWorkspaceSnapshot, ConfigWorkspaceDraftInput, ConfigWorkspaceOperation, ConfigWorkspaceSaveResult, ConfigWorkspaceRecoveryReview } from '$lib/types/gui-api';
export type EditorPhase = 'idle' | 'loading' | 'loaded' | 'editing' | 'validating' | 'planning' | 'planned' | 'saving' | 'saved' | 'applying' | 'checking' | 'completing' | 'applied' | 'unknown' | 'error';
export interface ValidationError { fieldPath?: string; message: string; }
export type { ConfigImpactItem, ConfigPlanApplyResult };
export interface WorkspacePorts {
  snapshot(): Promise<ConfigWorkspaceSnapshot>;
  plan(input: ConfigWorkspaceDraftInput): Promise<ConfigWorkspacePlan>;
  validate(input: ConfigWorkspaceDraftInput): Promise<{ sourceJson: string; effectiveJson: string }>;
  format(sourceJson: string): Promise<string>;
  save(input: ConfigWorkspaceDraftInput): Promise<ConfigWorkspaceSaveResult>;
  apply(input: { planId: string; strategy: 'hot_reload' | 'restart' }): Promise<ConfigWorkspaceOperation>;
  operation(planId: string): Promise<ConfigWorkspaceOperation>;
  discard(planId: string): Promise<void>;
  review(planId: string): Promise<ConfigWorkspaceRecoveryReview>;
  complete(input: { planId: string; reviewId: string }): Promise<ConfigWorkspaceOperation>;
}
const defaultPorts: WorkspacePorts = { snapshot: getConfigWorkspaceSnapshot, plan: planConfigWorkspace, validate: validateConfigWorkspace, format: formatConfigWorkspace, save: saveConfigWorkspaceLocal, apply: applyConfigWorkspace, operation: inspectConfigWorkspaceOperation, discard: discardConfigWorkspacePlan, review: reviewConfigWorkspaceRecovery, complete: completeConfigWorkspaceLocal };
const message = (error: unknown, fallback: string): string => {
  // Workspace backend errors use fixed public messages; do not stringify arbitrary error objects.
  return error && typeof error === 'object' && 'message' in error && typeof error.message === 'string' ? error.message : fallback;
};
export class ConfigWorkspaceService {
  phase = $state<EditorPhase>('idle');
  sourceJson = $state(''); draftJson = $state(''); localEditsJson = $state(''); effectiveJson = $state('');
  profileName = $state(''); dirty = $state(false); conflict = $state(false);
  readOnlyReason = $state<string | null>(null); pendingApply = $state(false);
  validationErrors = $state<ValidationError[]>([]); lastError = $state<string | null>(null);
  lastAppliedAt = $state<number | null>(null); planResult = $state<ConfigWorkspacePlan | null>(null);
  compositionLayers = $state<ConfigCompositionLayer[]>([]);
  runtime = $state<ConfigWorkspaceSnapshot['runtime'] | null>(null);
  lastTransaction = $state<ConfigTransactionReceipt | null>(null);
  supportsPlanApply = $state(true); operationId = $state<string | null>(null);
  operation = $state<ConfigWorkspaceOperation | null>(null);
  recoveryReview = $state<ConfigWorkspaceRecoveryReview | null>(null);
  private activeCalls = $state(0);
  private async tracked<T>(request: () => Promise<T>): Promise<T> { this.activeCalls++; try { return await request(); } finally { this.activeCalls--; } }
  private operationEditVersion = -1;
  private recoveryVersion = -1;
  private binding = $state<string | null>(null);
  private editVersion = 0; private readVersion = 0; private planVersion = -1;
  private readonly ports: WorkspacePorts;
  constructor(ports: WorkspacePorts = defaultPorts) {
    this.ports = ports;
    if (ports === defaultPorts) proxyConfigSignal.onActiveChanged(() => { void this.reconcileExternalSource(); });
  }
  get busy(): boolean { return this.activeCalls > 0 || ['loading', 'validating', 'planning', 'saving', 'applying', 'checking', 'completing'].includes(this.phase); }
  get writable(): boolean { return !!this.binding && !this.readOnlyReason && !this.conflict && !this.operationId; }
  private invalidatePlan(): void {
    const id = this.planResult?.planId; this.planResult = null; this.planVersion = -1;
    if (id && id !== this.operationId) void this.ports.discard(id).catch(() => {});
  }
  async load(discardDraft = false): Promise<boolean> {
    if (this.dirty && !discardDraft) { this.lastError = '草稿尚未保存；确认放弃草稿后才能刷新来源'; return false; }
    if (this.busy) return false;
    const read = ++this.readVersion, edit = this.editVersion;
    this.phase = 'loading';
    try {
      const snapshot = await this.ports.snapshot();
      if (read !== this.readVersion || edit !== this.editVersion) return false;
      this.acceptSnapshot(snapshot); this.phase = this.operationId ? 'unknown' : 'loaded'; return true;
    } catch (error) {
      if (read !== this.readVersion || edit !== this.editVersion) return false;
      this.lastError = message(error, '工作区读取失败；原草稿保留'); this.phase = 'error'; return false;
    }
  }
  private acceptSnapshot(snapshot: ConfigWorkspaceSnapshot): void {
    this.invalidatePlan(); this.editVersion++;
    this.binding = snapshot.binding ?? null; this.profileName = snapshot.profileName;
    this.readOnlyReason = snapshot.readOnlyReason ?? null;
    // Text is prepared by Rust. Never JSON.stringify a JS-parsed native u64 configuration.
    this.sourceJson = this.readOnlyReason ? '' : snapshot.sourceJson ?? '';
    this.draftJson = this.sourceJson; this.localEditsJson = this.readOnlyReason ? '' : snapshot.localEditsJson ?? '';
    this.effectiveJson = this.readOnlyReason ? '' : snapshot.effectiveJson ?? '';
    this.compositionLayers = snapshot.composition?.layers ?? [];
    this.runtime = snapshot.runtime; this.lastTransaction = snapshot.lastTransaction ?? null;
    this.operation = snapshot.pendingOperation ?? null;
    this.operationId = this.operation && !['confirmed', 'rejected'].includes(this.operation.state) ? this.operation.planId : null;
    this.operationEditVersion = this.editVersion; this.recoveryReview = null; this.recoveryVersion = -1;
    this.pendingApply = snapshot.pendingApply; this.dirty = false; this.conflict = false;
    this.validationErrors = []; this.lastError = null;
  }
  async reconcileExternalSource(): Promise<void> {
    if (this.phase === 'idle' && this.binding === null) return;
    const read = ++this.readVersion, edit = this.editVersion;
    try {
      const snapshot = await this.ports.snapshot();
      if (read !== this.readVersion || edit !== this.editVersion) return;
      if (snapshot.binding === this.binding && snapshot.readOnlyReason === this.readOnlyReason) return;
      this.invalidatePlan(); this.recoveryReview = null; this.recoveryVersion = -1;
      if (this.dirty || this.busy || this.operationId) {
        this.phase = this.operationId ? 'unknown' : 'editing';
        this.conflict = true; this.readOnlyReason = snapshot.readOnlyReason ?? null;
        this.lastError = '来源或运行代次已变化；原草稿保留，旧预检失效。请明确放弃草稿并刷新来源后重新预检';
      } else { this.acceptSnapshot(snapshot); this.phase = this.operationId ? 'unknown' : 'loaded'; }
    } catch {
      if (read === this.readVersion && edit === this.editVersion) { this.invalidatePlan(); this.recoveryReview = null; this.conflict = true; this.lastError = '来源读取未确认；原草稿保留，请重新核对'; }
    }
  }
  updateDraft(text: string): void {
    this.editVersion++; this.readVersion++; this.draftJson = text; this.dirty = text !== this.sourceJson;
    this.validationErrors = []; this.invalidatePlan(); this.recoveryReview = null; this.recoveryVersion = -1;
    this.phase = this.operationId ? 'unknown' : this.dirty ? 'editing' : 'loaded';
  }
  parseDraft(): Record<string, unknown> | null {
    // Syntax/root-object check only; this object is never formatted, saved or submitted.
    try { const value = JSON.parse(this.draftJson); return value && typeof value === 'object' && !Array.isArray(value) ? value : null; } catch { return null; }
  }
  private input(): ConfigWorkspaceDraftInput | null {
    if (this.busy) return null;
    if (!this.writable || !this.binding) { this.lastError = this.readOnlyReason ?? '来源冲突或原事务未确认；请保留草稿并核对，不能重复提交'; return null; }
    if (!this.parseDraft()) { this.validationErrors = [{ message: 'JSON 格式无效，根值必须是对象；原草稿保留' }]; return null; }
    return { binding: this.binding, sourceJson: this.draftJson };
  }
  private current(edit: number, binding: string): boolean { return edit === this.editVersion && binding === this.binding && !this.conflict; }
  async format(): Promise<boolean> {
    const edit = this.editVersion, text = this.draftJson;
    try { const formatted = await this.ports.format(text); if (edit !== this.editVersion) return false; this.updateDraft(formatted); return true; }
    catch (error) { if (edit === this.editVersion) this.lastError = message(error, '格式化失败；草稿保留'); return false; }
  }
  async validate(notifySuccess = true): Promise<boolean> {
    const input = this.input(); if (!input) return false;
    const edit = this.editVersion; this.readVersion++; this.phase = 'validating'; this.validationErrors = []; this.lastError = null;
    try {
      const result = await this.ports.validate(input); if (!this.current(edit, input.binding)) return false;
      this.effectiveJson = result.effectiveJson; this.phase = this.dirty ? 'editing' : 'loaded';
      if (notifySuccess) success('本地 Zero schema 和引用校验通过；未确认运行兼容性'); return true;
    } catch (error) {
      if (!this.current(edit, input.binding)) return false;
      this.validationErrors = normalizeConfigValidationError(error); this.lastError = message(error, '本地校验失败；草稿保留'); this.phase = 'editing'; return false;
    }
  }
  async planApply(): Promise<boolean> {
    const input = this.input(); if (!input) return false;
    const edit = this.editVersion; this.readVersion++; this.invalidatePlan(); this.phase = 'planning'; this.lastError = null;
    try {
      const plan = await this.ports.plan(input);
      if (!this.current(edit, input.binding) || plan.binding !== input.binding) { void this.ports.discard(plan.planId).catch(() => {}); return false; }
      this.planResult = plan; this.planVersion = edit; this.effectiveJson = plan.effectiveJson; this.phase = 'planned'; return true;
    } catch (error) {
      if (!this.current(edit, input.binding)) return false;
      this.lastError = message(error, '预检失败；草稿保留'); this.validationErrors = normalizeConfigValidationError(error); this.phase = 'editing'; return false;
    }
  }
  async saveLocal(): Promise<boolean> {
    const input = this.input(); if (!input) return false;
    const edit = this.editVersion, previousRuntime = this.runtime;
    this.readVersion++; this.phase = 'saving'; this.lastError = null;
    try {
      const result = await this.tracked(() => this.ports.save(input));
      if (!this.current(edit, input.binding)) return false;
      if (result.state !== 'saved_pending_apply') throw new Error('本地保存结果未确认');
      this.acceptSnapshot(result.snapshot); this.phase = 'saved'; this.pendingApply = true;
      if (previousRuntime) this.runtime = { ...previousRuntime, confirmed: false, reason: '已保存，待应用；此前读取的运行信息仅供核对，本次保存未查询或修改运行态' };
      success('已保存，待应用；未改变系统代理或 VPN'); return true;
    } catch (error) {
      if (!this.current(edit, input.binding)) return false;
      this.lastError = message(error, '本地保存未确认；草稿保留，不会自动重试'); this.validationErrors = normalizeConfigValidationError(error); this.phase = 'error'; return false;
    }
  }
  async apply(): Promise<boolean> {
    if (this.operationId) return this.checkOperation();
    if (!this.planResult && !await this.planApply()) return false;
    return this.doApply('hot_reload');
  }
  async confirmApply(): Promise<boolean> { return this.doApply('restart'); }
  private async doApply(strategy: 'hot_reload' | 'restart'): Promise<boolean> {
    const plan = this.planResult;
    if (!plan || this.planVersion !== this.editVersion || !this.writable || plan.binding !== this.binding) { this.lastError = '原预检已失效，请保留草稿并重新预检'; return false; }
    if (plan.applyUnavailableReason || !plan.runtimeValidated) { this.lastError = plan.applyUnavailableReason ?? '运行预检未确认；可保存本地待应用'; return false; }
    if (strategy === 'restart' || plan.requiresRestart.length) { this.lastError = '当前客户端安全启动及固定材料通道未适配；保留重启影响，可保存本地待应用'; return false; }
    const edit = this.editVersion; this.readVersion++; this.operationId = plan.planId; this.operationEditVersion = edit; this.operation = { planId: plan.planId, state: 'submitted_unknown' }; this.recoveryReview = null; this.phase = 'applying'; this.lastError = null;
    try {
      const operation = await this.tracked(() => this.ports.apply({ planId: plan.planId, strategy }));
      return await this.acceptOperation(operation, plan.planId, edit);
    } catch (error) {
      if (edit !== this.editVersion || this.operationId !== plan.planId || this.conflict) return false;
      if (error && typeof error === 'object' && 'code' in error && error.code === 'workspace_preflight_rejected') {
        this.operationId = null; this.invalidatePlan(); this.phase = 'editing'; this.lastError = '副作用前预检被拒绝，未发送运行变更；草稿保留，请重新预检'; return false;
      }
      // A lost response never creates a new plan or retries apply. Query only the original ID.
      if (edit === this.editVersion) { this.phase = 'unknown'; this.lastError = '原事务结果未确认；不会重放。请核对原操作'; }
      return false;
    }
  }
  async checkOperation(): Promise<boolean> {
    const id = this.operationId; if (!id || this.busy) return false;
    if (this.conflict) { this.lastError = '来源或运行代次已变化；原操作仅作历史。保留草稿，请明确刷新后再核对'; return false; }
    const edit = this.editVersion; this.phase = 'checking';
    try { return await this.acceptOperation(await this.tracked(() => this.ports.operation(id)), id, edit); }
    catch { if (id === this.operationId && edit === this.editVersion && !this.conflict) { this.phase = 'unknown'; this.lastError = '原事务仍未确认；不会根据相同内容的旧回执宣称成功，也不会重放'; } return false; }
  }
  async reviewRecovery(): Promise<boolean> {
    const id = this.operationId; if (!id || this.busy) return false;
    if (this.conflict) { this.lastError = '来源或运行代次已变化；原操作仅作历史。保留草稿，请明确刷新后再核对'; return false; }
    const edit = this.editVersion, binding = this.binding;
    this.recoveryReview = null; this.recoveryVersion = -1; this.phase = 'checking'; this.lastError = null;
    try {
      const review = await this.tracked(() => this.ports.review(id));
      if (id !== this.operationId || edit !== this.editVersion || binding !== this.binding || this.conflict) return false;
      if (review.planId !== id) throw new Error('恢复检查返回的原操作身份不一致');
      this.recoveryReview = review; this.recoveryVersion = edit; this.phase = 'unknown';
      return true;
    } catch (error) {
      if (id === this.operationId && edit === this.editVersion && binding === this.binding && !this.conflict) {
        this.phase = 'unknown'; this.lastError = message(error, '原操作恢复检查失败；不会生成新计划或重放运行变更');
      }
      return false;
    }
  }
  get canCompleteLocal(): boolean {
    const review = this.recoveryReview;
    return !this.busy && !this.dirty && !this.conflict && !!review?.canCompleteLocal && !!review.reviewId
      && review.planId === this.operationId && this.recoveryVersion === this.editVersion
      && !!review.expiresAtUnixMs && /^(0|[1-9]\d{0,19})$/.test(review.expiresAtUnixMs)
      && BigInt(review.expiresAtUnixMs) > BigInt(Date.now());
  }
  async completeLocal(reviewId: string): Promise<boolean> {
    const review = this.recoveryReview;
    if (!review || !this.canCompleteLocal || !review.reviewId || review.reviewId !== reviewId) {
      this.lastError = this.dirty ? '未保存草稿仍保留；明确放弃草稿后重新检查原操作' : '原恢复检查不可提交或已过期；请重新检查，不会重放运行操作'; return false;
    }
    const id = review.planId, edit = this.editVersion; this.operationEditVersion = edit;
    // A review is a single-use local publication permit, never a new apply plan.
    this.recoveryReview = null; this.recoveryVersion = -1; this.phase = 'completing'; this.lastError = null;
    try { return await this.acceptOperation(await this.tracked(() => this.ports.complete({ planId: id, reviewId: review.reviewId! })), id, edit); }
    catch (error) {
      if (id === this.operationId && edit === this.editVersion && !this.conflict) { this.phase = 'unknown'; this.lastError = message(error, '原本地发布结果未确认；请重新检查原操作，不会自动重复提交'); }
      return false;
    }
  }
  private async acceptOperation(operation: ConfigWorkspaceOperation, id: string, edit: number): Promise<boolean> {
    if (id !== this.operationId || edit !== this.editVersion || this.conflict) return false;
    if (operation.planId !== id) { this.phase = 'unknown'; this.lastError = '回执不属于原操作；保留原屏障，请重新核对'; return false; }
    this.operation = operation;
    if (this.operationEditVersion !== edit) { this.phase = 'unknown'; this.lastError = '原操作回执仅作为历史；新草稿保留，请明确放弃后重新核对'; return false; }
    if (operation.state === 'rejected') { this.operationId = null; this.invalidatePlan(); this.phase = 'editing'; this.lastError = operation.reason ?? '副作用前预检被拒绝；未发送运行变更，请重新预检'; return false; }
    if (operation.state !== 'confirmed' || operation.receipt?.state !== 'confirmed') { this.phase = 'unknown'; this.lastError = operation.reason ?? '原事务未确认；不会重放'; return false; }
    const receipt = operation.receipt;
    this.operationId = null;
    try {
      const snapshot = await this.ports.snapshot();
      if (edit !== this.editVersion || this.conflict || this.operationId !== null) return false;
      const currentReceipt = snapshot.profileId === receipt.profileId && snapshot.runtime.confirmed
        && snapshot.runtime.identity?.coreInstanceId === receipt.runtimeIdentity?.coreInstanceId
        && snapshot.runtime.identity.configRevision === receipt.runtimeIdentity?.configRevision
        && snapshot.runtime.effectiveDigest === receipt.effectiveDigest
        && snapshot.lastTransaction?.transactionId === receipt.transactionId;
      if (!currentReceipt) {
        this.conflict = true; this.phase = 'loaded';
        if (this.runtime) this.runtime = { ...this.runtime, confirmed: false, reason: '原操作已成为历史；当前运行状态需重新核对' };
        this.lastError = '原事务回执仅作为历史；当前来源或运行身份不再匹配，原草稿保留，请明确刷新后核对'; warning(this.lastError);
      } else {
        this.acceptSnapshot(snapshot); this.operation ??= operation;
        this.phase = this.operationId ? 'unknown' : 'applied';
        this.lastAppliedAt = receipt.completedAtUnixMs;
        if (!this.operationId) success('原工作区事务及运行身份已确认');
      }
    } catch { if (edit !== this.editVersion || this.conflict || this.operationId !== null) return false; this.conflict = true; this.phase = 'loaded'; this.lastError = '原事务回执已保存为历史，工作区刷新失败；保留草稿，请手动核对当前状态'; }
    return true;
  }
  reset(): void { this.updateDraft(this.sourceJson); this.lastError = null; }
}
export const configEditor = new ConfigWorkspaceService();
