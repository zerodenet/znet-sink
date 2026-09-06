/** A command may fail without rejecting: callers decide how to present feedback. */
export type CommandResult = { ok: true } | { ok: false; message: string };

export function requireCommandSuccess(result: CommandResult): void {
  if (!result.ok) throw new Error(result.message);
}

/** Disable service notifications when the calling view owns operation feedback. */
export interface CommandOptions { notify?: boolean }
