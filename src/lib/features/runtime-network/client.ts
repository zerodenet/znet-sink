import {invoke} from '@tauri-apps/api/core';
import type {NetworkPorts,NetworkSnapshot,NetworkPlan,NetworkStatus} from './types';
export const runtimeNetworkPorts:NetworkPorts={snapshot:()=>invoke<NetworkSnapshot>('gui_runtime_network_snapshot'),preview:input=>invoke<NetworkPlan>('gui_runtime_network_preview',{input}),save:planId=>invoke<NetworkStatus>('gui_runtime_network_save',{planId}),status:planId=>invoke<NetworkStatus>('gui_runtime_network_status',{planId}),discard:planId=>invoke('gui_runtime_network_discard',{planId})};
