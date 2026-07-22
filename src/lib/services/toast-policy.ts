import type { NotificationType } from '$lib/services/notification-log';

export const MAX_ACTIVE_TOASTS = 3;

export interface ToastAdmissionItem {
  id: number;
  type: NotificationType;
  message: string;
}

export interface ToastAdmissionPlan {
  duplicateId?: number;
  evictIds: number[];
}

export function planToastAdmission(
  active: Iterable<ToastAdmissionItem>,
  incoming: Pick<ToastAdmissionItem, 'type' | 'message'>,
): ToastAdmissionPlan {
  const items = Array.from(active);
  const duplicate = items.find(
    (item) => item.type === incoming.type && item.message === incoming.message,
  );
  if (duplicate) {
    return { duplicateId: duplicate.id, evictIds: [] };
  }

  const overflow = Math.max(0, items.length - MAX_ACTIVE_TOASTS + 1);
  return {
    evictIds: items.slice(0, overflow).map((item) => item.id),
  };
}
