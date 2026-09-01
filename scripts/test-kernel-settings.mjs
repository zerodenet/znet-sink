import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (path) => readFileSync(path, 'utf8');

const panel = read('src/lib/components/settings/CoreConfigPanel.svelte');
const service = read('src/lib/services/core.ts');
const commands = read('src-tauri/src/commands/app_config.rs');
const commandRegistry = read('src-tauri/src/lib.rs');
const model = read('src-tauri/src/models/app_config.rs');
const migration = read('src-tauri/src/services/kernel_settings.rs');
const overlay = read('src-tauri/src/services/rule_overlay.rs');

assert.ok(
  panel.includes('客户端内核配置迁移') &&
    panel.includes('importKernelSettings') &&
    panel.includes('exportKernelSettings') &&
    panel.includes('不会包含订阅代理配置') &&
    panel.includes('导入校验失败时不会覆盖当前配置'),
  'the kernel settings panel must expose the portable migration contract',
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
  importCommand.includes('app_config::replace(state.inner(), old_config.clone())') &&
    importCommand.includes('restart_core_and_restore_tun(app_handle.clone(), state.inner())'),
  'failed runtime application must restore both persisted settings and app-owned TUN',
);
assert.ok(
  migration.includes('"gui.app.v1"') &&
    migration.includes('unsupported client kernel settings schema') &&
    migration.includes('MAX_IMPORT_BYTES'),
  'import must support legacy projection and reject unsupported or oversized bundles',
);
assert.ok(
  overlay.includes('pub(crate) fn validate_app_config_candidate') &&
    overlay.includes('Some(app_config.url_test.tolerance_ms)') &&
    overlay.includes('Some(&app_config.dns)'),
  'candidate validation must use the imported DNS, routing, and URLTest settings',
);

console.log('kernel-settings: ok');
