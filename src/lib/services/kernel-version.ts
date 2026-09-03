import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  KernelVersionList,
  KernelInstallResult,
  KernelDownloadProgress,
  KernelVersionDetect,
} from '$lib/types/kernel-version';
import {
  RELEASE_CHECK_FAILURE_RETRY_MS,
  RELEASE_CHECK_INTERVAL_MS,
} from '$lib/services/release-check-policy';

export const KERNEL_VERSION_CHECK_INTERVAL_MS = RELEASE_CHECK_INTERVAL_MS;

let cachedVersionList: KernelVersionList | null = null;
let lastSuccessfulCheckAt = 0;
let lastFailedCheckAt = 0;
let lastCheckError: unknown = null;
let pendingVersionList: Promise<KernelVersionList> | null = null;

export async function listKernelVersions(
  { force = false }: { force?: boolean } = {},
): Promise<KernelVersionList> {
  const now = Date.now();
  if (
    !force
    && cachedVersionList
    && now - lastSuccessfulCheckAt < KERNEL_VERSION_CHECK_INTERVAL_MS
  ) {
    return cachedVersionList;
  }
  if (
    !force
    && lastCheckError !== null
    && now - lastFailedCheckAt < RELEASE_CHECK_FAILURE_RETRY_MS
  ) {
    if (cachedVersionList) return cachedVersionList;
    throw lastCheckError;
  }
  if (pendingVersionList) return pendingVersionList;

  const request = invoke<KernelVersionList>('kernel_list_versions');
  pendingVersionList = request;
  try {
    const result = await request;
    cachedVersionList = result;
    lastSuccessfulCheckAt = Date.now();
    lastCheckError = null;
    lastFailedCheckAt = 0;
    return result;
  } catch (error) {
    lastCheckError = error;
    lastFailedCheckAt = Date.now();
    if (!force && cachedVersionList) return cachedVersionList;
    throw error;
  } finally {
    if (pendingVersionList === request) pendingVersionList = null;
  }
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
