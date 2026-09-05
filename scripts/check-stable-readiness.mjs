import { execFileSync } from 'node:child_process';
import { assertReleaseDelta, readRecord, sourceFingerprint, validateRecord } from './stable-readiness.mjs';

try {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--fingerprint') {
    console.log(sourceFingerprint());
  } else {
    const released = args.length === 1 && args[0] === '--release';
    if (args.length && !released) throw new Error('Usage: check-stable-readiness.mjs [--fingerprint|--release]');
    if (execFileSync('git', ['status', '--porcelain']).toString().trim()) {
      throw new Error('Stable qualification requires a clean, committed source tree.');
    }
    if (released) assertReleaseDelta();
    const ref = released ? 'HEAD^' : 'HEAD';
    const errors = validateRecord(readRecord(ref), sourceFingerprint(ref));
    if (errors.length) {
      console.error(`Stable release blocked: ${errors.length} unmet requirements.`);
      console.error(errors.slice(0, 12).join('\n'));
      process.exitCode = 1;
    } else {
      console.log('Stable readiness evidence is complete for the paired source and all four platforms.');
    }
  }
} catch (error) {
  console.error(`Stable release blocked: ${error.message}`);
  process.exitCode = 1;
}
