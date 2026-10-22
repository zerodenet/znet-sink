import { invoke } from '@tauri-apps/api/core';
export type AndroidPluginJobs = {
  schemaVersion: 1;
  scheduling: { state: 'scheduled' | 'unavailable'; periodMs?: number | null; observedAtUnixMs?: string | null; reason: string } | null;
  lastRun: { runId: string; origin: 'android-job-scheduler'; state: 'running' | 'completed' | 'deferred' | 'failed' | 'stopped' | 'result-unknown'; startedAtUnixMs: string; endedAtUnixMs?: string | null; observedAtUnixMs?: string | null; completed: number | null; failed: number | null; unconfirmed: number | null; reason: string | null; resultConfirmed: boolean | null; androidStopReason?: number | null } | null;
};
export async function readAndroidPluginJobs(): Promise<AndroidPluginJobs> {
  return invoke<AndroidPluginJobs>('mobile_plugin_jobs_status');
}
export function jobReason(reason: string | null | undefined): string {
  const reasons: Record<string, string> = {
    'android-system-deferred-periodic-job': '已交由系统尽力调度',
    'android-job-definition-conflict': '系统作业标识冲突，请保留记录并反馈',
    'android-job-scheduling-unavailable': '系统暂未接受后台作业',
    'android-job-state-unavailable': '无法读取后台作业记录',
    'android-process-recreated': '进程已重建，原作业结果待核对',
    'android-system-stopped-job': '系统停止了本次作业，在途结果待核对',
    'android-job-time-budget-exhausted': '达到本次运行预算，在途结果待核对',
    'android-job-service-destroyed': '系统服务已销毁，在途结果待核对',
    'foreground-or-other-owner': '前台或另一作业正在处理，等待后续核对',
    'no-verified-plugin-catalog': '没有已验证的插件目录缓存',
    'no-enabled-due-tasks': '本次没有已启用且到期的任务',
    'stopped-before-start': '系统已在执行前停止本次作业',
    'unconfirmed-not-replayed': '存在未确认的任务，不会自动重放',
    'native-run-unconfirmed': '原生执行结果未确认',
  };
  return reason ? reasons[reason] ?? '本次作业未确认，请核对任务记录' : '—';
}
