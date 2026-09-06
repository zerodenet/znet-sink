// Exercise the production toast view and admission policy without native persistence.
import { SvelteMap } from 'svelte/reactivity';
import { planToastAdmission } from '$lib/services/toast-policy';
export type ToastType = 'success' | 'error' | 'warning' | 'info';
const toasts = new SvelteMap<number, { id:number; type:ToastType; message:string; duration:number }>();
let nextId = 0;
export function showToast(type:ToastType, message:string, duration = 4000) {
  const admission = planToastAdmission(toasts.values(), {type,message});
  if (admission.duplicateId !== undefined) return admission.duplicateId;
  for (const id of admission.evictIds) toasts.delete(id);
  const id = ++nextId;
  toasts.set(id, {id,type,message,duration});
  window.dispatchEvent(new CustomEvent('fixture-toast', {detail:message}));
  if (duration > 0) setTimeout(() => toasts.delete(id), duration);
  return id;
}
export const error = (message:string) => showToast('error', message);
export const success = (message:string) => showToast('success', message);
export const warning = (message:string) => showToast('warning', message);
export const getToasts = () => toasts;
export const dismissToast = (id:number) => { toasts.delete(id); };
