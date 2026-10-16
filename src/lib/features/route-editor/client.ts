import { invoke } from '@tauri-apps/api/core';
import type { RoutePorts, RouteSnapshot, RoutePlan, RouteStatus } from './types';
export const routeEditorPorts:RoutePorts={
 snapshot:(profileId,fromSource=false)=>invoke<RouteSnapshot>('gui_route_editor_snapshot',{profileId,fromSource}),
 preview:input=>invoke<RoutePlan>('gui_route_editor_preview',{input}),
 save:planId=>invoke<RouteStatus>('gui_route_editor_save',{planId}),
 status:planId=>invoke<RouteStatus>('gui_route_editor_status',{planId}),
 discard:planId=>invoke('gui_route_editor_discard',{planId}),
};
