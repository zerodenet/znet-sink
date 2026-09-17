import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
test('migrated subscription execution cannot restore its own HTTP backend', () => {
  const source = read('src-tauri/src/services/subscription.rs').split('#[cfg(test)]')[0];
  assert.doesNotMatch(source, /reqwest::|TcpStream::|Command::/);
});
test('VM execution and admission adapters cannot own native IO or kernel channels', () => {
  for (const file of ['runtime.rs', 'bridge.rs', 'policy.rs']) {
    assert.doesNotMatch(read(`src-tauri/crates/plugin-sandbox/src/${file}`), /reqwest::|std::fs|TcpStream|UnixStream|kernel::|std::process/);
  }
});
test('management remains runtime neutral and client execution cannot depend on plugin runtime', () => {
  assert.doesNotMatch(read('src-tauri/crates/client-core/Cargo.toml'), /reqwest|tauri|rquickjs|znet-plugin/);
  assert.doesNotMatch(read('src-tauri/crates/client-capabilities/Cargo.toml'), /tauri|rquickjs|znet-plugin|znet-engine/);
});
test('rules and configuration imports delegate migrated effects', () => {
  assert.doesNotMatch(read('src-tauri/src/services/rule_set.rs'), /reqwest::/);
  assert.match(read('src-tauri/src/services/rule_set.rs'), /files::publish/);
  const builtins = read('src-tauri/src/services/builtin_rules.rs').split('#[cfg(test)]')[0];
  assert.doesNotMatch(builtins, /reqwest::|\.send\(\)/);
  assert.match(builtins, /download::fetch_bounded/);
  assert.match(builtins, /files::publish/);
  const source = read('src-tauri/src/services/proxy_config.rs');
  assert.doesNotMatch(source, /std::fs|fs::read/);
  assert.match(source, /material::import/);
  const settings = read('src-tauri/src/services/kernel_settings.rs');
  assert.doesNotMatch(settings, /fs::read|fs::write/);
  assert.match(settings, /files::(?:read|publish)/);
});
test('automatic startup and activation never export full configuration', () => {
  for (const path of ['runtime_host/start.rs', 'capture/connection.rs', 'commands/config_workspace.rs', 'commands/gui_core/dns_apply.rs', 'commands/gui_core.rs', 'services/rule_overlay.rs', 'services/proxy_config.rs']) {
    assert.doesNotMatch(read(`src-tauri/src/${path}`), /export_active/);
  }
  assert.match(read('src-tauri/src/runtime_host/spawn.rs'), /apply_active_runtime/);
  assert.match(read('src-tauri/src/kernel/configuration.rs'), /config\.apply_runtime/);
  assert.doesNotMatch(read('src-tauri/src/kernel/zero/commands.rs'), /pub async fn apply_config/);
});

test('component admission and shutdown use the shared lifecycle owner', () => {
  assert.match(read('src-tauri/crates/plugin-sandbox/src/policy.rs'), /\.admit_component\(/);
  assert.doesNotMatch(read('src-tauri/crates/plugin-sandbox/src/policy.rs'), /\.admit\(/);
  for (const path of ['runtime_host/shutdown.rs', 'application/mod.rs']) {
    assert.match(read(`src-tauri/src/${path}`), /capabilities.*\.shutdown\(\)|capabilities\(\)\.shutdown\(\)/s);
  }
});

test('desktop host cannot enable experimental guest network or accept a custom registry', () => {
  const host = read('src-tauri/src/services/plugins.rs') + read('src-tauri/src/services/plugins/operations.rs');
  assert.doesNotMatch(host, /execute_network_lab|Remote::new|reqwest::/);
  assert.match(read('src-tauri/src/services/plugins/io.rs'), /network::get/);
  const commands = read('src-tauri/src/commands/plugins.rs');
  assert.doesNotMatch(commands, /directory_url|registry_url|source: String|public_key/);
  assert.match(commands, /plugins_supported/);
});

test('native downloads and probes cannot restore private HTTP executors', () => {
  for (const path of [
    'services/kernel_manager.rs',
    'services/network_probe.rs',
    'services/download/mod.rs',
    'services/download/transfer.rs',
    'services/plugins/io.rs',
    'commands/app_update/download.rs',
  ]) {
    assert.doesNotMatch(read(`src-tauri/src/${path}`), /reqwest::blocking::Client|reqwest::Proxy|\.send\(\)/);
  }
  assert.match(read('src-tauri/src/services/download/transfer.rs'), /network::stream_get/);
  assert.match(read('src-tauri/src/services/network_probe.rs'), /network::get/);
});

test('desktop UI routes opener and clipboard effects through host commands', () => {
  const componentPaths = [
    'components/settings/CoreConfigPanel.svelte',
    'components/settings/AboutPanel.svelte',
    'components/settings/AppConfigPanel.svelte',
    'components/tabs/RulesTab.svelte',
    'components/tabs/PluginsTab.svelte',
    'components/tabs/PluginCatalog.svelte',
    'components/tabs/ConnectionInspectorWorkspace.svelte',
  ];
  for (const path of componentPaths) {
    assert.doesNotMatch(read(`src/lib/${path}`), /@tauri-apps\/plugin-opener/);
  }
  assert.match(read('src/lib/services/platform.ts'), /platform_open_url/);
  assert.match(read('src/lib/services/clipboard.ts'), /platform_clipboard_write/);
});

test('system mutations and diagnostic jobs are admitted as native operations', () => {
  for (const path of ['commands/system_proxy.rs', 'commands/gui_core.rs', 'commands/proxy_mode.rs', 'commands/tool_jobs.rs']) {
    assert.match(read(`src-tauri/src/${path}`), /native_operation::execute_async/);
  }
  assert.match(read('src-tauri/src/services/tool_jobs.rs'), /tool_job_store::save/);
  assert.match(read('src-tauri/src/application/mod.rs'), /tool_job_store::load/);
  assert.match(read('src-tauri/src/commands/tool_jobs.rs'), /job:\{\}:\{:\?\}/);
  assert.match(read('src-tauri/src/services/probe/manual.rs'), /native_operation::execute_async/);
  assert.match(read('src-tauri/src/services/probe/runtime.rs'), /probe_job_store::save/);
  assert.match(read('src-tauri/src/application/mod.rs'), /probe_job_store::load/);
});

test('persistent logs and exports sanitize data and use managed operations', () => {
  assert.match(read('src-tauri/src/services/logs.rs'), /redaction::(?:text|sensitive)/);
  assert.match(read('src-tauri/src/commands/debug.rs'), /native_operation::execute_async/);
  assert.match(read('src-tauri/src/commands/logs.rs'), /native_operation::execute_async/);
});
