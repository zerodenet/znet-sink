import {
  getCoreConfig,
  getCoreRuntime,
  getCoreStats,
  guiValidateConfig,
  guiApplyConfig,
  guiPlanApplyConfig,
  getGuiZeroCapabilities,
} from '$lib/services/core';
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
  /** The authoritative config snapshot from the kernel (read-only reference). */
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

  /** Load the current config from the kernel as the authoritative source.
   *  Also probes kernel capabilities to determine plan_apply support. */
  async load(): Promise<void> {
    this.phase = 'idle';
    this.lastError = null;
    this.validationErrors = [];
    this.planResult = null;

    try {
      const [configResult, capsResult] = await Promise.allSettled([
        getCoreConfig(),
        getGuiZeroCapabilities(),
      ]);

      // Probe capabilities
      if (capsResult.status === 'fulfilled' && capsResult.value.available) {
        this.supportsPlanApply = capsResult.value.features.includes('config.plan_apply');
      } else {
        this.supportsPlanApply = false;
      }

      // Load config
      const config = configResult.status === 'fulfilled' ? configResult.value : null;
      if (!config) {
        this.lastError = '内核不可用，无法加载配置';
        this.phase = 'error';
        return;
      }

      this._sourceObj = config;
      this.sourceJson = JSON.stringify(config, null, 2);
      this.draftJson = this.sourceJson;
      this.dirty = false;
      this.phase = 'loaded';
    } catch (e) {
      this.lastError = e instanceof Error ? e.message : String(e);
      this.phase = 'error';
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
  async validate(): Promise<boolean> {
    const config = this.parseDraft();
    if (!config) {
      this.validationErrors = [{ message: 'JSON 格式无效，请检查语法' }];
      return false;
    }

    this.phase = 'validating';
    this.validationErrors = [];
    this.lastError = null;

    try {
      const result = await guiValidateConfig(config) as Record<string, unknown>;

      // Kernel returns { ok: true, result: ... } or an error
      if (result['ok'] === false) {
        const error = result['error'] as Record<string, unknown> | undefined;
        this.validationErrors = [{
          fieldPath: error?.['field_path'] as string | undefined,
          message: (error?.['message'] as string) ?? '内核校验失败',
        }];
        this.phase = 'editing';
        return false;
      }

      // Check if result contains validation issues
      const validationResult = result['result'] ?? result;
      if (typeof validationResult === 'object' && validationResult !== null) {
        const vr = validationResult as Record<string, unknown>;
        if (vr['valid'] === false) {
          const errors = Array.isArray(vr['errors']) ? vr['errors'] : [];
          this.validationErrors = errors.map((e: unknown) => {
            const err = e as Record<string, unknown>;
            return {
              fieldPath: err['field_path'] as string | undefined,
              message: (err['message'] as string) ?? String(e),
            };
          });
          this.phase = 'editing';
          return false;
        }
      }

      this.phase = 'editing'; // Still editing, just validated OK
      return true;
    } catch (e) {
      this.lastError = e instanceof Error ? e.message : String(e);
      this.validationErrors = [{ message: this.lastError }];
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
      this.lastError = e instanceof Error ? e.message : String(e);
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

    // Step 1: Validate
    this.phase = 'validating';
    try {
      const valid = await this.validate();
      if (!valid) return false;
    } catch (e) {
      this.lastError = `校验失败: ${e instanceof Error ? e.message : String(e)}`;
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
        this.lastError = (error?.['message'] as string) ?? '内核应用配置失败';
        this.phase = 'error';
        warning(`配置应用失败: ${this.lastError}`);
        return false;
      }

      this.lastAppliedAt = Date.now();
      this.phase = 'applied';
      success('配置已热加载到运行中的内核');
    } catch (e) {
      this.lastError = e instanceof Error ? e.message : String(e);
      this.phase = 'error';
      warning(`配置应用失败: ${this.lastError}`);
      return false;
    }

    // Reconcile — reload the authoritative config
    await this.reconcile();
    return true;
  }

  /** Re-query config + runtime from the kernel after apply to verify state. */
  private async reconcile(): Promise<void> {
    try {
      // Re-fetch config to get the authoritative snapshot
      const [configResult] = await Promise.allSettled([
        getCoreConfig(),
        // Also refresh runtime and stats in parallel so the UI is up-to-date
        getCoreRuntime(),
        getCoreStats(),
      ]);

      if (configResult.status === 'fulfilled') {
        const config = configResult.value;
        this._sourceObj = config;
        this.sourceJson = JSON.stringify(config, null, 2);
        this.draftJson = this.sourceJson;
        this.dirty = false;
        this.phase = 'loaded';
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
  }
}

export const configEditor = new ConfigEditorService();
