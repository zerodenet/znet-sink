import {invoke} from '@tauri-apps/api/core';
import type {FlowCloseInventory,FlowClosePreview,FlowCloseResult,FlowCloseStatus,FlowClosePorts} from './types';
export const flowClosePorts:FlowClosePorts={
 inventory:()=>invoke<FlowCloseInventory>('gui_flow_close_inventory'),
 preview:input=>invoke<FlowClosePreview>('gui_flow_close_preview',{input}),
 submit:planId=>invoke<FlowCloseResult>('gui_flow_close_submit',{planId}),
 status:planId=>invoke<FlowCloseStatus>('gui_flow_close_status',{planId}),
 discard:id=>invoke('gui_flow_close_discard',{id}),
};
