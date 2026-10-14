import type { SubscriptionProfile } from '$lib/types/domain';

export type SchedulePlatform = 'mobile-unadapted' | 'mobile-foreground' | 'desktop' | 'unknown';
type ScheduleInput = Pick<SubscriptionProfile, 'enabled' | 'updateIntervalSecs' | 'managedSource'>;

/** Saved policy is independent of an actual scheduled/finished system task. */
export function subscriptionScheduling(profile: ScheduleInput, platform: SchedulePlatform): string | null {
  if (platform === 'desktop') return null;
  if (profile.managedSource) {
    return platform === 'unknown'
      ? '来源插件的调度状态未确认。'
      : '调度由来源插件管理；移动插件执行与后台调度尚未适配。';
  }
  const interval = profile.updateIntervalSecs;
  if (interval !== undefined && (!Number.isSafeInteger(interval) || interval < 60)) {
    return '自动同步周期数据无法确认，请重新检查来源设置。';
  }
  if (!profile.enabled) return '已暂停参与同步；已有缓存仍保留。';
  if (interval === undefined) return '仅手动同步；没有自动同步周期意图。';
  if (platform === 'unknown') return `已保存 ${interval} 秒周期意图；平台调度状态未确认。`;
  if (platform === 'mobile-foreground') return `已保存 ${interval} 秒周期意图；启动或回到前台时核对到期任务，最近执行和结果以实际记录为准。移动系统后台调度尚未适配，不承诺锁屏或睡眠期间定点执行。`;
  return `已保存 ${interval} 秒周期意图；下一次自动执行未安排，移动系统后台调度尚未适配。最近成功同步可能来自手动操作，不代表自动任务已经执行。`;
}
