import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

const targets = JSON.parse(readFileSync(new URL('./android-build-targets.json', import.meta.url), 'utf8'));

export function androidBuildTarget(name, profile = 'debug') {
  const target = Object.hasOwn(targets, name) ? targets[name] : null;
  // Independent Release distribution remains the existing arm64 contract.
  if (!target || !['debug', 'release'].includes(profile) || (name === 'i686' && profile !== 'debug')) {
    throw new Error('Unsupported Android inventory target/profile');
  }
  return {...target};
}

export function androidInventoryTarget(triple, profile) {
  const name = Object.keys(targets).find(key => targets[key].triple === triple);
  return androidBuildTarget(name, profile);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const target = androidBuildTarget(process.argv[2] || 'aarch64');
  console.log(`${target.triple} ${target.abi} ${target.gradleArch}`);
}
