import {applyProfileSettings, type ProfileSettings} from '$lib/services/core';
export type {ProfileSettings} from '$lib/services/core';

/** Diff the loaded form, so unchanged fields continue to follow the subscription. */
export async function saveProfileSettings(snapshot: ProfileSettings, patch: Record<string, unknown>): Promise<ProfileSettings> {
  const changes: Record<string,unknown> = {};
  const settings = snapshot.settings as unknown as Record<string,unknown>;
  for (const [section,value] of Object.entries(patch)) {
    if (section === 'dns' || section === 'bypass') {
      if ((section === 'bypass' && snapshot.sourceBypass != null && !snapshot.editedFields.includes('bypass')) || JSON.stringify(value) !== JSON.stringify(settings[section])) changes[section] = value;
    } else {
      const previous = settings[section] as Record<string,unknown>;
      for (const [field,next] of Object.entries(value as Record<string,unknown>)) {
        if (JSON.stringify(next) !== JSON.stringify(previous?.[field])) changes[`${section}.${field}`] = next;
      }
    }
  }
  return applyProfileSettings(snapshot.profileId, changes);
}
export async function restoreProfileSettings(snapshot: ProfileSettings, prefix: string): Promise<ProfileSettings> {
  const keys = snapshot.editedFields.filter(key=>key === prefix || key.startsWith(`${prefix}.`));
  return applyProfileSettings(snapshot.profileId, {}, keys);
}
export function isLocallyEdited(snapshot: ProfileSettings | null, prefix: string): boolean {
  return !!snapshot?.editedFields.some(key=>key === prefix || key.startsWith(`${prefix}.`));
}
