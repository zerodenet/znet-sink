import assert from 'node:assert/strict';
import {
  CURRENT_ONBOARDING_VERSION,
  ONBOARDING_STORAGE_KEY,
  completeOnboarding,
  isOnboardingRequired,
  resetOnboarding,
} from '../src/lib/services/onboarding.ts';

class MemoryStorage {
  values = new Map();

  getItem(key) {
    return this.values.get(key) ?? null;
  }

  setItem(key, value) {
    this.values.set(key, value);
  }

  removeItem(key) {
    this.values.delete(key);
  }
}

{
  const storage = new MemoryStorage();
  assert.equal(isOnboardingRequired(storage), true, 'first launch must show onboarding');

  completeOnboarding(storage);
  assert.equal(storage.getItem(ONBOARDING_STORAGE_KEY), CURRENT_ONBOARDING_VERSION);
  assert.equal(isOnboardingRequired(storage), false, 'current onboarding must stay completed');

  resetOnboarding(storage);
  assert.equal(isOnboardingRequired(storage), true, 'reset must show onboarding again');
}

{
  const storage = new MemoryStorage();
  storage.setItem(ONBOARDING_STORAGE_KEY, '1');
  assert.equal(isOnboardingRequired(storage), true, 'a newer onboarding version must be shown once');
}

{
  const storage = new MemoryStorage();
  completeOnboarding(storage);
  storage.setItem('znet-reset', '1');
  assert.equal(isOnboardingRequired(storage), true, 'legacy reset must still be honored');
  assert.equal(storage.getItem('znet-reset'), null, 'legacy reset marker must be consumed');
  assert.equal(storage.getItem(ONBOARDING_STORAGE_KEY), null);
}

console.log('onboarding state tests passed');
