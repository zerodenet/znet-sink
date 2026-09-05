import { compareAppVersions } from '$lib/services/app-update-policy';
import type { KernelInstallStage, KernelRelease } from '$lib/types/kernel-version';

export const installStageLabels: Record<KernelInstallStage, string> = {
  preparing: '正在准备下载…',
  validating: '正在校验安装包与配置…',
  backing_up: '正在备份当前版本…',
  installing: '正在安装内核…',
  starting: '正在启动内核并恢复连接…',
  rolling_back: '升级未完成，正在恢复原版本…',
};

export function newestStableKernel(releases: KernelRelease[]): KernelRelease | null {
  return releases.filter((release) => release.channel === 'stable')
    .sort((a, b) => compareAppVersions(b.version, a.version))[0] ?? null;
}

export function isKernelUpdate(current: string | null, candidate: string | null): boolean {
  const version = /^v?\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?(?:\+[\da-zA-Z.-]+)?$/;
  return Boolean(current && candidate && version.test(current) && version.test(candidate)
    && compareAppVersions(candidate, current) > 0);
}
