import {
  applyConfigWorkspace,
  getConfigWorkspaceSnapshot,
  guiValidateConfig,
  planConfigWorkspace,
} from '$lib/services/core';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
import {
  getConfigEditorErrorMessage,
  normalizeConfigValidationError,
  normalizeConfigValidationResponse,
} from '$lib/services/config-validation';
import { success, warning } from '$lib/services/toast.svelte';
import { guiState } from '$lib/services/gui-state.svelte';
import type {
  ConfigCompositionLayer,
  ConfigImpactItem,
  ConfigPlanApplyResult,
  ConfigTransactionReceipt,
  ConfigWorkspacePlan,
  ConfigWorkspaceSnapshot,
} from '$lib/types/gui-api';

export type EditorPhase =
  | 'idle'
  | 'loaded'
  | 'editing'
  | 'validating'
  | 'planning'
  | 'planned'
  | 'applying'
  | 'applied'
  | 'error';

export interface ValidationError {
  fieldPath?: string;
  message: string;
}

export type { ConfigImpactItem, ConfigPlanApplyResult };

class ConfigWorkspaceService {
  phase = $state<EditorPhase>('idle');
  sourceJson = $state('');
  draftJson = $state('');
  localEditsJson = $state('{}');
  effectiveJson = $state('{}');
  profileName = $state('');
  dirty = $state(false);
  validationErrors = $state<ValidationError[]>([]);
  lastError = $state<string | null>(null);
  lastAppliedAt = $state<number | null>(null);
  planResult = $state<ConfigWorkspacePlan | null>(null);
  compositionLayers = $state<ConfigCompositionLayer[]>([]);
  runtime = $state<ConfigWorkspaceSnapshot['runtime'] | null>(null);
  lastTransaction = $state<ConfigTransactionReceipt | null>(null);
  supportsPlanApply = $state(true);

  private sourceProfileId: string | null = null;
  private sourceProfileUpdatedAt: number | null = null;

  constructor() {
    proxyConfigSignal.onActiveChanged(() => void this.reconcileExternalSource());
  }

  async load(): Promise<boolean> {
    this.phase = 'idle';
    this.lastError = null;
    this.validationErrors = [];
    this.planResult = null;
    try {
      this.acceptSnapshot(await getConfigWorkspaceSnapshot());
      this.phase = 'loaded';
      return true;
    } catch (error) {
      this.lastError = getConfigEditorErrorMessage(error, '无法读取配置工作区');
      this.phase = 'error';
      return false;
    }
  }

  private acceptSnapshot(snapshot: ConfigWorkspaceSnapshot): void {
    this.sourceProfileId = snapshot.profileId;
    this.sourceProfileUpdatedAt = snapshot.sourceUpdatedAtUnixMs;
    this.profileName = snapshot.profileName;
    this.sourceJson = JSON.stringify(snapshot.sourceConfig, null, 2);
    this.draftJson = this.sourceJson;
    this.localEditsJson = JSON.stringify(snapshot.localEdits, null, 2);
    this.effectiveJson = JSON.stringify(snapshot.effectiveConfig, null, 2);
    this.compositionLayers = snapshot.composition?.layers ?? [];
    this.runtime = snapshot.runtime;
    this.lastTransaction = snapshot.lastTransaction ?? null;
    this.dirty = false;
  }

  private async reconcileExternalSource(): Promise<void> {
    if (this.phase === 'idle' && this.sourceProfileId === null) return;
    try {
      const snapshot = await getConfigWorkspaceSnapshot();
      if (
        snapshot.profileId !== this.sourceProfileId
        || snapshot.sourceUpdatedAtUnixMs !== this.sourceProfileUpdatedAt
      ) {
        this.acceptSnapshot(snapshot);
        this.phase = 'loaded';
      }
    } catch {
      // A later mutation or explicit refresh retries reconciliation.
    }
  }

  updateDraft(json: string): void {
    this.draftJson = json;
    this.dirty = json !== this.sourceJson;
    this.validationErrors = [];
    if (this.dirty) this.planResult = null;
    this.phase = this.dirty ? 'editing' : 'loaded';
  }

  parseDraft(): Record<string, unknown> | null {
    try {
      const parsed = JSON.parse(this.draftJson);
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
        ? parsed as Record<string, unknown>
        : null;
    } catch {
      return null;
    }
  }

  async validate(notifySuccess = true): Promise<boolean> {
    const config = this.parseDraft();
    if (!config) {
      this.validationErrors = [{ message: 'JSON 格式无效，请检查语法' }];
      return false;
    }
    this.phase = 'validating';
    this.validationErrors = [];
    this.lastError = null;
    try {
      const result = normalizeConfigValidationResponse(await guiValidateConfig(config));
      if (!result.valid) {
        this.validationErrors = result.errors;
        this.phase = 'editing';
        return false;
      }
      this.phase = this.dirty ? 'editing' : 'loaded';
      if (notifySuccess) success('配置校验通过');
      return true;
    } catch (error) {
      this.lastError = getConfigEditorErrorMessage(error, '内核校验失败');
      this.validationErrors = normalizeConfigValidationError(error);
      this.phase = 'editing';
      return false;
    }
  }

  async planApply(): Promise<boolean> {
    const config = this.parseDraft();
    if (!config) {
      this.validationErrors = [{ message: 'JSON 格式无效，请检查语法' }];
      return false;
    }
    this.phase = 'planning';
    this.planResult = null;
    this.lastError = null;
    try {
      const result = await planConfigWorkspace(config);
      this.planResult = result;
      this.effectiveJson = JSON.stringify(result.effectiveConfig, null, 2);
      this.phase = 'planned';
      return true;
    } catch (error) {
      this.lastError = getConfigEditorErrorMessage(error, '配置预检失败');
      this.phase = 'editing';
      return false;
    }
  }

  async apply(): Promise<boolean> {
    if (!await this.validate(false)) return false;
    if (!await this.planApply()) return false;
    if (this.planResult && this.planResult.requiresRestart.length > 0) return false;
    return this.doApply('hot_reload');
  }

  async confirmApply(): Promise<boolean> {
    return this.doApply('restart');
  }

  private async doApply(strategy: 'hot_reload' | 'restart'): Promise<boolean> {
    const sourceConfig = this.parseDraft();
    if (!sourceConfig || !this.sourceProfileId || this.sourceProfileUpdatedAt === null) {
      this.lastError = '配置来源已变化，请刷新后重试';
      this.phase = 'error';
      return false;
    }
    this.phase = 'applying';
    this.lastError = null;
    try {
      const receipt = await applyConfigWorkspace({
        profileId: this.sourceProfileId,
        sourceUpdatedAtUnixMs: this.sourceProfileUpdatedAt,
        sourceConfig,
        strategy,
      });
      this.lastAppliedAt = receipt.completedAtUnixMs;
      const reconciled = await this.load();
      this.phase = 'applied';
      if (reconciled && this.runtime?.confirmed) {
        success(strategy === 'restart' ? '配置已重启应用并完成运行态确认' : '配置已热加载并完成运行态确认');
      } else {
        this.lastTransaction = receipt;
        warning(reconciled
          ? '配置事务已完成，但运行态随后发生变化；请刷新并核对当前配置'
          : '配置已由内核确认应用，但工作区刷新失败；请手动刷新核对界面');
      }
      await guiState.refreshNodeStateAfterConfigChange();
      return true;
    } catch (error) {
      // Losing the desktop response after commit must not trigger a replay.
      // Recover the backend receipt and live identity before showing failure.
      try {
        const snapshot = await getConfigWorkspaceSnapshot();
        if (
          snapshot.runtime.confirmed
          && snapshot.lastTransaction?.profileId === this.sourceProfileId
          && snapshot.lastTransaction.effectiveDigest === this.planResult?.effectiveDigest
        ) {
          this.acceptSnapshot(snapshot);
          this.lastAppliedAt = snapshot.lastTransaction.completedAtUnixMs;
          this.phase = 'applied';
          success('配置事务已完成，已从运行态恢复确认结果');
          await guiState.refreshNodeStateAfterConfigChange();
          return true;
        }
      } catch {
        // Preserve the original transaction error when recovery cannot read.
      }
      this.lastError = getConfigEditorErrorMessage(error, '配置事务失败');
      this.phase = 'error';
      warning(`配置事务失败: ${this.lastError}`);
      return false;
    }
  }

  reset(): void {
    this.draftJson = this.sourceJson;
    this.dirty = false;
    this.validationErrors = [];
    this.lastError = null;
    this.planResult = null;
    this.phase = 'loaded';
  }
}

export const configEditor = new ConfigWorkspaceService();
