export const ONBOARDING_STORAGE_KEY = 'znet-onboarding-version';
export const CURRENT_ONBOARDING_VERSION = '2';

const LEGACY_RESET_STORAGE_KEY = 'znet-reset';

export interface OnboardingStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function isOnboardingRequired(storage: OnboardingStorage): boolean {
  if (storage.getItem(LEGACY_RESET_STORAGE_KEY) === '1') {
    storage.removeItem(LEGACY_RESET_STORAGE_KEY);
    storage.removeItem(ONBOARDING_STORAGE_KEY);
    return true;
  }

  return storage.getItem(ONBOARDING_STORAGE_KEY) !== CURRENT_ONBOARDING_VERSION;
}

export function completeOnboarding(storage: OnboardingStorage): void {
  storage.setItem(ONBOARDING_STORAGE_KEY, CURRENT_ONBOARDING_VERSION);
}

export function resetOnboarding(storage: OnboardingStorage): void {
  storage.removeItem(ONBOARDING_STORAGE_KEY);
}
