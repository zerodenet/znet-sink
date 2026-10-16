import { PluginUninstallSession } from './state';
import { pluginUninstallPorts } from './client';
// UI projection of the current host operation, not an execution queue or durable store.
// Host recent/status remains the recovery source after this module/process disappears.
export const pluginUninstallSession = new PluginUninstallSession(pluginUninstallPorts);
