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
  const source = read('src-tauri/src/services/proxy_config.rs');
  assert.doesNotMatch(source, /std::fs|fs::read/);
  assert.match(source, /material::import/);
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
    assert.match(read(`src-tauri/src/${path}`), /shutdown_components\(\)/);
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
