import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (path) => readFileSync(path, 'utf8');

const versionPanel = read('src/lib/components/settings/CoreConfigPanel.svelte');
const versionCard = read('src/lib/components/core/KernelVersionCard.svelte');
const draggableModal = read('src/lib/components/DraggableModal.svelte');
const kernelVersionService = read('src/lib/services/kernel-version.ts');
const releaseCheckPolicy = read('src/lib/services/release-check-policy.ts');
const updaterService = read('src/lib/services/updater.svelte.ts');
const advancedPanel = read('src/lib/components/settings/ConfigEditorPanel.svelte');
const transfer = read('src/lib/components/settings/KernelSettingsTransfer.svelte');
const service = read('src/lib/services/core.ts');
const commands = read('src-tauri/src/commands/app_config.rs');
const commandRegistry = read('src-tauri/src/lib.rs');
const model = read('src-tauri/src/models/app_config.rs');
const migration = read('src-tauri/src/services/kernel_settings.rs');
const overlay = read('src-tauri/src/services/rule_overlay.rs');
const tunRuntime = read('src-tauri/src/kernel/zero/runtime.rs');
const tunPanel = read('src/lib/components/settings/TunSettingsPanel.svelte');

assert.ok(
  advancedPanel.includes('<KernelSettingsTransfer />') &&
    transfer.includes('配置迁移') &&
    transfer.includes('importSettings') &&
    transfer.includes('exportSettings') &&
    transfer.includes('导入或导出 DNS、TUN 和客户端运行偏好'),
  'advanced settings must expose the portable migration actions with concise copy',
);
assert.ok(
  !versionPanel.includes('KernelSettingsTransfer') &&
    !versionPanel.includes('importClientKernelSettings') &&
    !versionPanel.includes('exportClientKernelSettings'),
  'kernel version management must not contain client settings migration',
);
assert.ok(
  draggableModal.includes('destroy()') && draggableModal.includes('node.remove()'),
  'portaled version dialogs must release their document.body node when navigation unmounts the owner',
);
assert.ok(
  releaseCheckPolicy.includes('RELEASE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000') &&
    kernelVersionService.includes('cachedVersionList') &&
    kernelVersionService.includes('pendingVersionList') &&
    kernelVersionService.includes('RELEASE_CHECK_FAILURE_RETRY_MS') &&
    updaterService.includes('UPDATE_CHECK_INTERVAL_MS = RELEASE_CHECK_INTERVAL_MS') &&
    updaterService.includes("document.visibilityState === 'visible') this.runScheduledCheck();") &&
    versionPanel.includes('listKernelVersions({ force })') &&
    versionPanel.includes('onclick={() => loadVersions(true)}') &&
    versionCard.includes('更新信息暂不可用'),
  'app and kernel release checks must share a six-hour cadence, cache results, deduplicate requests, and reserve bypass for manual refresh',
);
assert.ok(
  service.includes("invoke('app_config_import_kernel_settings'") &&
    service.includes("invoke('app_config_export_kernel_settings'"),
  'the frontend service must use typed import/export commands',
);
assert.ok(
  commandRegistry.includes('app_config_import_kernel_settings') &&
    commandRegistry.includes('app_config_export_kernel_settings'),
  'Tauri must register both kernel settings commands',
);
assert.match(model, /CLIENT_KERNEL_SETTINGS_SCHEMA: &str = "znet\.client-kernel-settings\.v1"/);
assert.match(model, /pub struct ClientKernelSettings \{/);
assert.match(model, /pub core: PortableCoreConfig/);
assert.match(model, /pub tun: AppTunConfig/);
assert.match(model, /pub dns: AppDnsConfig/);
assert.match(model, /pub routing: AppRoutingConfig/);
assert.match(model, /pub url_test: AppUrlTestConfig/);
assert.match(model, /pub include_cidrs: Vec<String>/);
assert.match(model, /pub exclude_cidrs: Vec<String>/);
assert.match(
  model.slice(model.indexOf('impl AppTunConfig'), model.indexOf('impl Default for AppLocalProxyConfig')),
  /enabled: Some\(false\)/,
  'new installs must keep privileged TUN explicitly disabled while retaining prepared defaults',
);
assert.doesNotMatch(
  model.slice(model.indexOf('pub struct PortableCoreConfig'), model.indexOf('impl ClientKernelSettings')),
  /executable_path|working_dir|config_path|socket|download_url/,
  'portable settings must exclude machine-bound paths and sockets',
);
const importCommand = commands.slice(commands.indexOf('pub async fn app_config_import_kernel_settings'));
assert.ok(
  importCommand.indexOf('kernel_settings::import_from_path(&old_config, path)?') <
    importCommand.indexOf('rule_overlay::validate_app_config_candidate(state.inner(), &new_config)?') &&
    importCommand.indexOf('rule_overlay::validate_app_config_candidate(state.inner(), &new_config)?') <
    importCommand.indexOf('app_config::replace(state.inner(), new_config.clone())?'),
  'import must parse and compose a detached candidate against the active profile before persisting it',
);
assert.ok(
  tunRuntime.includes('"include_cidrs"') &&
    tunRuntime.includes('"exclude_cidrs"') &&
    tunPanel.includes('TUN 接管网段') &&
    tunPanel.includes('TUN 排除网段'),
  'portable TUN route inclusion and exclusion must reach tun.start and remain manageable',
);
assert.ok(
  importCommand.includes('app_config::replace(state.inner(), old_config.clone())') &&
    importCommand.includes('legacy_tun_runtime_enabled') &&
    importCommand.includes('rollback_tun_override') &&
    importCommand.includes('restart_core_and_restore_tun('),
  'failed runtime application must restore both persisted settings and app-owned TUN',
);
assert.ok(
  migration.includes('"gui.app.v1"') &&
    migration.includes('unsupported client kernel settings schema') &&
    migration.includes('MAX_IMPORT_BYTES') &&
    migration.includes('is_contiguous_mask') &&
    migration.includes('validate_tun_owned_addresses'),
  'import must support legacy projection and reject unsupported or oversized bundles',
);
assert.ok(
  overlay.includes('pub(crate) fn validate_app_config_candidate') &&
    overlay.includes('Some(app_config.url_test.tolerance_ms)') &&
    overlay.includes('Some(&app_config.dns)'),
  'candidate validation must use the imported DNS, routing, and URLTest settings',
);

console.log('kernel-settings: ok');
