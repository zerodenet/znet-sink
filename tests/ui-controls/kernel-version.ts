import type { KernelDownloadProgress, KernelInstallStage, KernelInstallProgress } from '../../src/lib/types/kernel-version';
const downloads = new Set<(progress: KernelDownloadProgress) => void>();
const stages = new Set<(progress: KernelInstallProgress) => void>();
export const detectKernelVersion = async () => ({ version:'0.0.17-rc.1', executableExists:true });
export const listKernelVersions = async () => {
  if (new URLSearchParams(location.search).get('scenario') === 'list-error') throw { message:'发布服务器暂不可用' };
  return { versions:[{ version:'0.0.16', channel:'stable', assetDownloadUrl:'https://example.test/zero.tar.gz', prerelease:false }] };
};
export const onDownloadProgress = async (callback: (progress: KernelDownloadProgress) => void) => {
  downloads.add(callback); return () => { downloads.delete(callback); };
};
export const onInstallProgress = async (callback: (progress: KernelInstallProgress) => void) => {
  stages.add(callback); return () => { stages.delete(callback); };
};
export const installKernelVersion = async (version: string) => {
  window.dispatchEvent(new Event('fixture-install-request'));
  for (const callback of downloads) callback({ version, bytesDownloaded:100, bytesTotal:100, percent:100 });
  return new Promise((resolve, reject) => {
    const stage = (event: Event) => {
      for (const callback of stages) callback({ version, stage:(event as CustomEvent<KernelInstallStage>).detail });
    };
    window.addEventListener('fixture-install-stage', stage);
    window.addEventListener('fixture-install-finish', (event) => {
      window.removeEventListener('fixture-install-stage', stage);
      if ((event as CustomEvent).detail === 'failure') reject({ code:'kernel_upgrade_failed', message:'新内核启动失败，已恢复原版本和连接。备份保存在 /fixture/backup' });
      else resolve({ success:true, version, executablePath:'/fixture/zero', checksumVerified:true, channel:'stable', message:'installed' });
    }, { once:true });
  });
};
