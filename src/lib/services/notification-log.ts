import type { LogAppend, LogLevel } from '$lib/types/logs';

export type NotificationType = 'success' | 'error' | 'warning' | 'info';

export function notificationLogLevel(type: NotificationType): LogLevel {
  switch (type) {
    case 'error':
      return 'error';
    case 'warning':
      return 'warn';
    case 'success':
    case 'info':
      return 'info';
  }
}

/** Build the persistent record for one user-visible notification.
 * The full message is deliberately preserved even when the transient UI has
 * limited space. */
export function buildNotificationLogInput(options: {
  id: number;
  type: NotificationType;
  message: string;
  duration: number;
}): LogAppend {
  return {
    source: 'app',
    level: notificationLogLevel(options.type),
    message: options.message,
    fields: {
      schema: 'znet.notification.v1',
      area: 'ui',
      operation: 'notification.show',
      notificationId: options.id,
      notificationType: options.type,
      durationMs: options.duration,
      placement: 'app-header-center',
    },
  };
}
