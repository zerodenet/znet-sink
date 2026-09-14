import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const backend = readFileSync(new URL('../src-tauri/src/commands/config_workspace.rs', import.meta.url), 'utf8');
const commands = readFileSync(new URL('../src-tauri/src/application/commands.rs', import.meta.url), 'utf8');
const client = readFileSync(new URL('../src/lib/services/config-workspace.svelte.ts', import.meta.url), 'utf8');
const panel = readFileSync(new URL('../src/lib/components/settings/ConfigEditorPanel.svelte', import.meta.url), 'utf8');
const core = readFileSync(new URL('../src/lib/services/core.ts', import.meta.url), 'utf8');

test('configuration workspace exposes one backend-owned plan and apply transaction', () => {
  assert.match(commands, /gui_config_workspace_snapshot/);
  assert.match(commands, /gui_config_workspace_plan/);
  assert.match(commands, /gui_config_workspace_apply/);
  assert.match(backend, /proxy_config_operation\(\)\.lock\(\)\.await/);
  assert.match(backend, /source_updated_at_unix_ms/);
  assert.match(backend, /config_apply_uncertain/);
  assert.match(backend, /rollback_hot/);
  assert.match(backend, /rollback_restart/);
});

test('client chooses real hot reload or restart only after a successful plan', () => {
  assert.match(client, /doApply\('hot_reload'\)/);
  assert.match(client, /doApply\('restart'\)/);
  assert.match(client, /if \(!await this\.planApply\(\)\) return false/);
  assert.match(client, /snapshot\.lastTransaction\.effectiveDigest === this\.planResult\?\.effectiveDigest/);
  assert.equal((client.match(/applyConfigWorkspace\(/g) ?? []).length, 1);
  assert.doesNotMatch(client, /proceeding to apply/);
  assert.doesNotMatch(core, /gui_plan_apply_config/);
});

test('workspace presents source, client overrides, effective config, and runtime confirmation', () => {
  assert.match(panel, /来源编辑/);
  assert.match(panel, /客户端覆盖/);
  assert.match(panel, /最终生效/);
  assert.match(panel, /运行态已确认/);
  assert.match(panel, /compositionLayers/);
});
