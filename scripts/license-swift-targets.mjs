#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { argumentsOf } from './license-common.mjs';

export function simulatorTriplesFromBuildLog(log) {
  if (!/\*\* BUILD SUCCEEDED \*\*/.test(log)) throw new Error('No successful Xcode build observed');
  const architectures = new Set();
  for (const match of log.matchAll(/(?:^|\s)-target\s+(arm64|x86_64)-apple-ios[\d.]+-simulator\b/g)) {
    architectures.add(match[1]);
  }
  // Xcode can hide driver arguments. Compile records still provide actual architectures when
  // the same log identifies its Simulator SDK; the runner machine architecture is never used.
  if (/(?:^|\s)-sdk\s+iphonesimulator\b|SDKROOT\s*=\s*iphonesimulator/.test(log)) {
    for (const match of log.matchAll(/\b(?:SwiftCompile|SwiftDriver Compilation)\b[^\r\n]*?\bnormal\s+(arm64|x86_64)\b/g)) {
      architectures.add(match[1]);
    }
  }
  if (!architectures.size) throw new Error('No actual iOS Simulator compiler target or SDK-qualified compile architecture observed');
  return [...architectures].sort().map((architecture) => architecture === 'arm64'
    ? 'aarch64-apple-ios-sim' : 'x86_64-apple-ios');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = argumentsOf(process.argv.slice(2));
  if (!args['build-log']) throw new Error('Required: --build-log PATH');
  process.stdout.write(`${simulatorTriplesFromBuildLog(fs.readFileSync(args['build-log'], 'utf8')).join('\n')}\n`);
}
