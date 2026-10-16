import { invoke } from '@tauri-apps/api/core';
export interface NotificationSnapshot {
  enabled:boolean;expectedRevision:string;permission:'granted'|'denied'|'prompt'|'prompt-with-rationale'|'unknown'|'unsupported';
  reason:string|null;presentation:'unconfirmed';
}
export const permissionLabel = (permission:NotificationSnapshot['permission']):string => ({granted:'已授权',denied:'已拒绝或关闭',prompt:'未请求授权','prompt-with-rationale':'需要说明后授权',unknown:'未确认',unsupported:'当前宿主不适用'})[permission];
export async function notificationOperation(command:'mobile_notification_status'|'mobile_notification_request'|'mobile_notification_preference',args?:{expectedRevision:string;enabled:boolean}):Promise<NotificationSnapshot>{
  const value=await invoke<NotificationSnapshot>(command,args);
  if(!value||typeof value.enabled!=='boolean'||typeof value.expectedRevision!=='string'||!/^[0-9a-f]{64}$/.test(value.expectedRevision)||!Object.hasOwn({granted:1,denied:1,prompt:1,'prompt-with-rationale':1,unknown:1,unsupported:1},value.permission)||value.presentation!=='unconfirmed'){
    throw new Error('通知偏好或系统状态回执无法确认，请重新核对');
  }
  return value;
}
