import { execFileSync } from 'node:child_process';
import { assertReleaseDelta, readRecord, sourceFingerprint, validateRecord, validateInitialReleaseWaiver } from './stable-readiness.mjs';

try {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--fingerprint') {
    console.log(sourceFingerprint());
  } else {
    const reset = args.length === 1 && args[0] === '--initial-release';
    const released = reset || (args.length === 1 && args[0] === '--release');
    if (args.length && !released) throw new Error('Usage: check-stable-readiness.mjs [--fingerprint|--release|--initial-release]');
    if (execFileSync('git', ['status', '--porcelain']).toString().trim()) {
      throw new Error('Stable qualification requires a clean, committed source tree.');
    }
    if (released) assertReleaseDelta();
    const ref = released ? 'HEAD^' : 'HEAD';
    const errors = reset
      ? validateInitialReleaseWaiver(readRecord(ref), sourceFingerprint(ref), process.env.GITHUB_REF_NAME)
      : validateRecord(readRecord(ref), sourceFingerprint(ref));
    if (errors.length) {
      console.error(`Stable release blocked: ${errors.length} unmet requirements.`);
      console.error(errors.slice(0, 12).join('\n'));
      process.exitCode = 1;
    } else {
      console.log(reset
        ? 'Owner-authorized v0.0.1 installation waiver verified; installed-e2e remains unqualified.'
        : 'Stable readiness evidence is complete for the paired source and all four platforms.');
    }
  }
} catch (error) {
  console.error(`Stable release blocked: ${error.message}`);
  process.exitCode = 1;
}
