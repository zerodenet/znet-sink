import {
  getCoreRuntime,
  getCoreStats,
  guiValidateConfig,
  guiApplyConfig,
  guiPlanApplyConfig,
  getGuiZeroCapabilities,
} from '$lib/services/core';
import { listProxyConfigs } from '$lib/services/config';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
import {
  getConfigEditorErrorMessage,
  normalizeConfigValidationError,
  normalizeConfigValidationResponse,
} from '$lib/services/config-validation';
import { success, warning } from '$lib/services/toast.svelte';
import { guiState } from '$lib/services/gui-state.svelte';
import type { ConfigPlanApplyResult, ConfigImpactItem } from '$lib/types/gui-api';

// ── Types ──

export type EditorPhase =
  | 'idle'      // No draft loaded
  | 'loaded'    // Draft loaded, user can edit
  | 'editing'   // Draft differs from source
  | 'validating' // Running config.validate
  | 'planning'  // Running config.plan_apply
  | 'planned'   // Plan result available, awaiting user confirmation
  | 'applying'  // Running config.apply
  | 'applied'   // Successfully applied, reconciling
  | 'error';    // Validation or apply failed

export interface ValidationError {
  fieldPath?: string;
  message: string;
}

export type { ConfigPlanApplyResult, ConfigImpactItem };

export interface ConfigEditorState {
  phase: EditorPhase;
  /** The active persisted Zero base config used as the editable reference. */
  sourceJson: string;
  /** The user's local draft (editable). */
  draftJson: string;
  /** Whether the draft has unsaved changes relative to source. */
  dirty: boolean;
  /** Validation errors returned by the kernel. Empty if not yet validated or if valid. */
  validationErrors: ValidationError[];
  /** Error message if apply or load failed. */
  lastError: string | null;
  /** Timestamp of the last successful apply (for reconciliation display). */
  lastAppliedAt: number | null;
  /** Plan-apply impact result (null until planApply is called). */
  planResult: ConfigPlanApplyResult | null;
}

function editableConfig(content: unknown): Record<string, unknown> | null {
  return typeof content === 'object' && content !== null && !Array.isArray(content)
    ? content as Record<string, unknown>
    : null;
}

// ── Service ──

class ConfigEditorService {
  phase = $state<EditorPhase>('idle');
  sourceJson = $state('');
  draftJson = $state('');
  dirty = $state(false);
  validationErrors = $state<ValidationError[]>([]);
  lastError = $state<string | null>(null);
  lastAppliedAt = $state<number | null>(null);
  planResult = $state<ConfigPlanApplyResult | null>(null);
  /** Whether the connected kernel supports `config.plan_apply`. */
  supportsPlanApply = $state(false);

  private _sourceObj: Record<string, unknown> | null = null;
  private _sourceProfileId: string | null = null;
  private _sourceProfileUpdatedAt: number | null = null;

  constructor() {
    // Lite and Pro mutate the same persisted proxy-config collection. Keep the
    // editor attached to that authoritative active profile instead of letting
    // its singleton draft silently survive a profile switch in another mode.
    proxyConfigSignal.onActiveChanged(() => {
      void this.reconcileExternalSource();
    });
  }

  get snapshot(): ConfigEditorState {
    return {
      phase: this.phase,
      sourceJson: this.sourceJson,
      draftJson: this.draftJson,
      dirty: this.dirty,
      validationErrors: this.validationErrors,
      lastError: this.lastError,
      lastAppliedAt: this.lastAppliedAt,
      planResult: this.planResult,
    };
  }

  /** Load the active persisted Zero config as the editable source.
   *  `core_get_config` intentionally returns a read-only ConfigSnapshot and
   *  must never be fed back into `config.validate` / `config.apply`.
   *  Also probes kernel capabilities to determine plan_apply support. */
  async load(): Promise<void> {
    this.phase = 'idle';
    this.lastError = null;
    this.validationErrors = [];
    this.planResult = null;

    try {
      const [profilesResult, capsResult] = await Promise.allSettled([
        listProxyConfigs(),
        getGuiZeroCapabilities(),
      ]);

      // Probe capabilities
      if (capsResult.status === 'fulfilled' && capsResult.value.available) {
        this.supportsPlanApply = capsResult.value.features.includes('config.plan_apply');
      } else {
        this.supportsPlanApply = false;
      }

      if (profilesResult.status === 'rejected') {
        this.lastError = getConfigEditorErrorMessage(profilesResult.reason, '无法读取活动配置');
        this.phase = 'error';
        return;
      }

      const activeProfile = profilesResult.value.find((profile) => profile.active);
      if (!activeProfile) {
        this._sourceProfileId = null;
        this._sourceProfileUpdatedAt = null;
        this.lastError = '当前没有活动代理配置';
        this.phase = 'error';
        return;
      }

      const config = editableConfig(activeProfile.content);
      if (!config) {
        this._sourceProfileId = activeProfile.id;
        this._sourceProfileUpdatedAt = activeProfile.updatedAtUnixMs;
        this.lastError = '当前活动代理配置没有可编辑的 Zero JSON 内容';
        this.phase = 'error';
        return;
      }

      this._sourceProfileId = activeProfile.id;
      this._sourceProfileUpdatedAt = activeProfile.updatedAtUnixMs;
      this._sourceObj = config;
      this.sourceJson = JSON.stringify(config, null, 2);
      this.draftJson = this.sourceJson;
      this.dirty = false;
      this.phase = 'loaded';
    } catch (e) {
      this.lastError = getConfigEditorErrorMessage(e, '无法读取活动配置');
      this.phase = 'error';
    }
  }

  /**
   * Reconcile this singleton editor after another surface mutates proxy
   * profiles. Unrelated non-active profile edits preserve the current draft;
   * an active id/content revision change reloads the authoritative source.
   */
  private async reconcileExternalSource(): Promise<void> {
    if (this.phase === 'idle' && this._sourceProfileId === null) return;

    try {
      const profiles = await listProxyConfigs();
      const activeProfile = profiles.find((profile) => profile.active) ?? null;
      const activeId = activeProfile?.id ?? null;
      const activeUpdatedAt = activeProfile?.updatedAtUnixMs ?? null;

      if (
        activeId === this._sourceProfileId
        && activeUpdatedAt === this._sourceProfileUpdatedAt
      ) {
        return;
      }

      await this.load();
    } catch {
      // Keep the last known editor state on a transient reconciliation error.
      // A later profile mutation or explicit refresh will retry the comparison.
    }
  }

  /** Called when the user edits the draft JSON. */
  updateDraft(json: string): void {
    this.draftJson = json;
    const dirty = json !== this.sourceJson;
    this.dirty = dirty;
    this.validationErrors = [];
    // Clear plan result when draft changes so it must be re-planned
    if (dirty) {
      this.planResult = null;
    }
    if (this.phase === 'applied' || this.phase === 'error' || this.phase === 'planned') {
      this.phase = this.dirty ? 'editing' : 'loaded';
    } else if (this.dirty) {
      this.phase = 'editing';
    } else {
      this.phase = 'loaded';
    }
  }

  /** Parse the current draft JSON. Returns null if invalid JSON. */
  parseDraft(): Record<string, unknown> | null {
    try {
      const parsed = JSON.parse(this.draftJson);
      if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
        return parsed as Record<string, unknown>;
      }
      return null;
    } catch {
      return null;
    }
  }

  /** Validate the current draft against the kernel without applying. */
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

      this.phase = 'editing'; // Still editing, just validated OK
      if (notifySuccess) {
        success('配置校验通过');
      }
      return true;
    } catch (e) {
      this.lastError = getConfigEditorErrorMessage(e, '内核校验失败');
      this.validationErrors = normalizeConfigValidationError(e);
      this.phase = 'editing';
      return false;
    }
  }

  /** Dry-run apply: analyse the impact of the current draft on the running kernel.
   *
   * On success transitions to `planned` and stores the result in `planResult`.
   * The UI can then show a confirmation dialog before calling `apply()`.
   */
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
      const result = await guiPlanApplyConfig(config);

      if (!result.valid) {
        this.validationErrors = result.errors.map((msg) => ({ message: msg }));
        this.planResult = result;
        this.phase = 'editing';
        return false;
      }

      this.planResult = result;
      this.phase = 'planned';
      return true;
    } catch (e) {
      this.lastError = getConfigEditorErrorMessage(e, '配置预检失败');
      this.phase = 'editing';
      return false;
    }
  }

  /** Apply the draft to the running kernel.
   *
   * When the kernel supports `config.plan_apply`:
   *   validate → planApply → if restart needed → pause at 'planned' (returns false)
   *   validate → planApply → no restart        → apply immediately
   * When unsupported: validate → apply directly.
   *
   * After a restart-impact pause, call `confirmApply()` to proceed.
   */
  async apply(): Promise<boolean> {
    const config = this.parseDraft();
    if (!config) {
      this.validationErrors = [{ message: 'JSON 格式无效，请检查语法' }];
      return false;
    }

    // Step 1: Validate. Applying has its own success notification, so avoid
    // showing a second toast when this validation is only a prerequisite.
    this.phase = 'validating';
    try {
      const valid = await this.validate(false);
      if (!valid) return false;
    } catch (e) {
      this.lastError = `校验失败: ${getConfigEditorErrorMessage(e, '内核校验失败')}`;
      this.phase = 'error';
      return false;
    }

    // Step 2: Plan (if kernel supports it)
    if (this.supportsPlanApply) {
      this.phase = 'planning';
      try {
        const planned = await this.planApply();
        if (!planned) return false;

        // If restart is needed, pause for user confirmation
        if (this.planResult && this.planResult.requiresRestart.length > 0) {
          // Phase is already 'planned' from planApply()
          return false;
        }
      } catch (e) {
        // Plan failed — fall through to apply anyway
        console.warn('[ConfigEditor] plan_apply failed, proceeding to apply:', e);
      }
    }

    // Step 3: Apply
    return this.doApply(config);
  }

  /** Confirm and execute the apply after user reviews restart impact.
   *  Only meaningful when phase is 'planned' with restart-impacting changes.
   */
  async confirmApply(): Promise<boolean> {
    const config = this.parseDraft();
    if (!config) {
      this.validationErrors = [{ message: 'JSON 格式无效，请检查语法' }];
      return false;
    }
    return this.doApply(config);
  }

  /** Internal: send the config to the kernel and reconcile. */
  private async doApply(config: Record<string, unknown>): Promise<boolean> {
    this.phase = 'applying';
    try {
      const result = await guiApplyConfig(config) as Record<string, unknown>;

      if (result['ok'] === false) {
        const error = result['error'] as Record<string, unknown> | undefined;
        this.lastError = typeof error?.['message'] === 'string'
          ? error['message']
          : '内核应用配置失败';
        this.phase = 'error';
        warning(`配置应用失败: ${this.lastError}`);
        return false;
      }

      this.lastAppliedAt = Date.now();
      this.phase = 'applied';
      success('配置已热加载到运行中的内核');
    } catch (e) {
      this.lastError = getConfigEditorErrorMessage(e, '内核应用配置失败');
      this.phase = 'error';
      warning(`配置应用失败: ${this.lastError}`);
      return false;
    }

    // Reconcile against the persisted active base config that gui_apply_config
    // updates after the kernel accepts the effective configuration.
    await this.reconcile();
    return true;
  }

  /** Re-query the active base config plus runtime state after apply. */
  private async reconcile(): Promise<void> {
    try {
      const [profilesResult] = await Promise.allSettled([
        listProxyConfigs(),
        // Also refresh runtime and stats in parallel so the UI is up-to-date
        getCoreRuntime(),
        getCoreStats(),
      ]);

      if (profilesResult.status === 'fulfilled') {
        const activeProfile = profilesResult.value.find((profile) => profile.active);
        const config = editableConfig(activeProfile?.content);
        if (activeProfile && config) {
          this._sourceProfileId = activeProfile.id;
          this._sourceProfileUpdatedAt = activeProfile.updatedAtUnixMs;
          this._sourceObj = config;
          this.sourceJson = JSON.stringify(config, null, 2);
          this.draftJson = this.sourceJson;
          this.dirty = false;
          this.phase = 'loaded';
        }
      }

      // Refresh config-derived GUI state (node list, policy sidebar) so the
      // node page reflects the freshly applied configuration.
      await guiState.refreshNodeStateAfterConfigChange();
    } catch {
      // Reconciliation is best-effort; don't change phase
    }
  }

  /** Reset the draft back to the source (discard unsaved changes). */
  reset(): void {
    this.draftJson = this.sourceJson;
    this.dirty = false;
    this.validationErrors = [];
    this.lastError = null;
    this.planResult = null;
    this.phase = 'loaded';
  }

  /** Clear the editor state entirely. */
  clear(): void {
    this.phase = 'idle';
    this.sourceJson = '';
    this.draftJson = '';
    this.dirty = false;
    this.validationErrors = [];
    this.lastError = null;
    this.lastAppliedAt = null;
    this.planResult = null;
    this._sourceObj = null;
    this._sourceProfileId = null;
    this._sourceProfileUpdatedAt = null;
  }
}

export const configEditor = new ConfigEditorService();