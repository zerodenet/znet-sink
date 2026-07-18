import { invoke } from '@tauri-apps/api/core';
import { SvelteMap } from 'svelte/reactivity';
import {
  buildNotificationLogInput,
  type NotificationType,
} from '$lib/services/notification-log';

export type ToastType = NotificationType;

export interface Toast {
  id: number;
  type: ToastType;
  message: string;
  duration: number;
}

let nextId = 0;
const toasts = new SvelteMap<number, Toast>();

export function showToast(type: ToastType, message: string, duration: number = 4000): number {
  const id = ++nextId;
  const toast = { id, type, message, duration };
  toasts.set(id, toast);

  // Notifications are transient UI. Persist the complete text independently
  // so errors and warnings remain inspectable after the banner disappears.
  void invoke('logs_append', {
    input: buildNotificationLogInput(toast),
  }).catch((logError) => {
    // Logging must never prevent the notification itself from being shown.
    console.error('[notification] failed to persist notification', logError);
  });

  if (duration > 0) {
    setTimeout(() => {
      toasts.delete(id);
    }, duration);
  }

  return id;
}

export function dismissToast(id: number): void {
  toasts.delete(id);
}

export function success(message: string, duration?: number): number {
  return showToast('success', message, duration);
}

export function error(message: string, duration?: number): number {
  return showToast('error', message, duration);
}

export function warning(message: string, duration?: number): number {
  return showToast('warning', message, duration);
}

export function info(message: string, duration?: number): number {
  return showToast('info', message, duration);
}

export function getToasts() {
  return toasts;
}
