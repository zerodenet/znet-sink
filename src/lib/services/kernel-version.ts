import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  KernelVersionList,
  KernelInstallResult,
  KernelDownloadProgress,
  KernelVersionDetect,
} from '$lib/types/kernel-version';

export async function listKernelVersions(): Promise<KernelVersionList> {
  return invoke('kernel_list_versions');
}

export async function installKernelVersion(
  version: string,
  downloadUrl: string,
  expectedSha256?: string,
  installDir?: string,
): Promise<KernelInstallResult> {
  return invoke('kernel_install_version', {
    version,
    downloadUrl,
    expectedSha256,
    installDir,
  });
}

export async function detectKernelVersion(): Promise<KernelVersionDetect> {
  return invoke('kernel_detect_version');
}

export function onDownloadProgress(
  callback: (progress: KernelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<KernelDownloadProgress>('kernel:download-progress', (event) => {
    callback(event.payload);
  });
}
