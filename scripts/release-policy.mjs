#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const CORE_PATTERN = '(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)';
const REQUEST_PATTERN = new RegExp(`^(${CORE_PATTERN})(?:-(rc))?$`);
const MANAGED_TAG_PATTERN = new RegExp(`^v(${CORE_PATTERN})(?:-(dev|rc)\\.([0-9]+))?$`);

function fail(message) {
  throw new Error(message);
}

function coreParts(version) {
  const match = new RegExp(`^${CORE_PATTERN}$`).exec(version);
  if (!match) fail(`invalid base version '${version}'`);
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

export function compareBaseVersions(left, right) {
  const a = coreParts(left);
  const b = coreParts(right);
  for (let i = 0; i < 3; i += 1) {
    if (a[i] !== b[i]) return a[i] > b[i] ? 1 : -1;
  }
  return 0;
}

export function parseRequestedVersion(input) {
  if (input.startsWith('v')) {
    fail(`version input must not include the 'v' tag prefix: '${input}'`);
  }
  const match = REQUEST_PATTERN.exec(input);
  if (!match) {
    fail(`invalid release request '${input}'; expected X.Y.Z or X.Y.Z-rc`);
  }
  return { baseVersion: match[1], requestedChannel: match[5] === 'rc' ? 'rc' : 'stable' };
}

export function parseManagedTag(tag) {
  const normalized = tag.replace(/^refs\/tags\//, '');
  const match = MANAGED_TAG_PATTERN.exec(normalized);
  if (!match) return null;
  return {
    tag: normalized,
    baseVersion: match[1],
    channel: match[5] ?? 'stable',
    buildId: match[6] ?? null,
  };
}

export function formatReleaseTimestamp(date = new Date()) {
  const pad = (value) => String(value).padStart(2, '0');
  return `${date.getUTCFullYear()}${pad(date.getUTCMonth() + 1)}${pad(date.getUTCDate())}${pad(date.getUTCHours())}${pad(date.getUTCMinutes())}`;
}

function managedState(tags) {
  const parsed = [...new Set(tags)].map(parseManagedTag).filter(Boolean);
  const stable = parsed.filter((item) => item.channel === 'stable');
  const latestStable = stable
    .map((item) => item.baseVersion)
    .sort(compareBaseVersions)
    .at(-1) ?? null;

  const active = parsed.filter((item) =>
    item.channel !== 'stable'
    && (!latestStable || compareBaseVersions(item.baseVersion, latestStable) > 0));
  const activeBases = [...new Set(active.map((item) => item.baseVersion))].sort(compareBaseVersions);
  if (activeBases.length > 1) {
    fail(`multiple active release lines found: ${activeBases.join(', ')}`);
  }
  return { parsed, latestStable, activeBase: activeBases[0] ?? null };
}

function assertBuildNumber(buildNumber) {
  if (!Number.isInteger(buildNumber) || buildNumber < 1 || buildNumber > 65535) {
    fail(`native build number '${buildNumber}' must be an integer between 1 and 65535`);
  }
}

export function resolveReleasePlan({ branch, input, tags = [], now = new Date(), buildNumber }) {
  assertBuildNumber(buildNumber);
  if (branch !== 'develop' && branch !== 'main') {
    fail(`releases are only allowed from develop or main; current branch is '${branch}'`);
  }

  const request = parseRequestedVersion(input);
  const state = managedState(tags);
  const sameBase = state.parsed.filter((item) => item.baseVersion === request.baseVersion);

  if (state.latestStable && compareBaseVersions(request.baseVersion, state.latestStable) <= 0) {
    fail(`release line ${request.baseVersion} is sealed by stable ${state.latestStable}; historical versions cannot be reopened`);
  }
  if (state.activeBase && request.baseVersion !== state.activeBase) {
    fail(`active release line is ${state.activeBase}; cannot branch release history into ${request.baseVersion}`);
  }

  let channel;
  if (branch === 'develop') {
    if (request.requestedChannel !== 'stable') {
      fail('develop accepts only X.Y.Z; the script adds the dev suffix automatically');
    }
    if (sameBase.some((item) => item.channel === 'rc')) {
      fail(`release line ${request.baseVersion} already reached rc and cannot return to dev`);
    }
    channel = 'dev';
  } else if (request.requestedChannel === 'rc') {
    if (!state.activeBase) {
      fail(`rc ${request.baseVersion} requires an existing dev release from develop`);
    }
    if (!sameBase.some((item) => item.channel === 'dev' || item.channel === 'rc')) {
      fail(`rc ${request.baseVersion} requires an existing dev release`);
    }
    channel = 'rc';
  } else {
    if (!state.activeBase || !sameBase.some((item) => item.channel === 'rc')) {
      fail(`stable ${request.baseVersion} requires an existing rc release`);
    }
    channel = 'stable';
  }

  const timestamp = formatReleaseTimestamp(now);
  const releaseVersion = channel === 'stable'
    ? request.baseVersion
    : `${request.baseVersion}-${channel}.${timestamp}`;
  const tag = `v${releaseVersion}`;
  if (state.parsed.some((item) => item.tag === tag)) {
    fail(`release tag ${tag} already exists; wait for the next UTC minute`);
  }

  return {
    branch,
    input,
    baseVersion: request.baseVersion,
    channel,
    releaseVersion,
    tag,
    timestamp: channel === 'stable' ? null : timestamp,
    buildNumber,
    latestStable: state.latestStable,
  };
}

function compareBuildIds(left, right) {
  const a = BigInt(left ?? '0');
  const b = BigInt(right ?? '0');
  return a === b ? 0 : a > b ? 1 : -1;
}

export function validatePublishedRelease({ branch, tag, tags = [] }) {
  const published = parseManagedTag(tag);
  if (!published) fail(`published tag '${tag}' is not managed by the release policy`);
  if (branch !== 'develop' && branch !== 'main') fail(`invalid release branch '${branch}'`);
  if (published.channel === 'dev' && branch !== 'develop') fail(`dev release ${tag} must originate from develop`);
  if (published.channel !== 'dev' && branch !== 'main') fail(`${published.channel} release ${tag} must originate from main`);

  const otherTags = tags.filter((candidate) => candidate.replace(/^refs\/tags\//, '') !== published.tag);
  const state = managedState(otherTags);
  const sameBase = state.parsed.filter((item) => item.baseVersion === published.baseVersion);

  if (state.latestStable && compareBaseVersions(published.baseVersion, state.latestStable) <= 0) {
    fail(`release line ${published.baseVersion} is sealed by stable ${state.latestStable}`);
  }
  if (state.activeBase && state.activeBase !== published.baseVersion) {
    fail(`active release line is ${state.activeBase}; cannot publish ${published.baseVersion}`);
  }
  if (published.channel === 'dev' && sameBase.some((item) => item.channel === 'rc')) {
    fail(`release line ${published.baseVersion} already reached rc and cannot return to dev`);
  }
  if (published.channel === 'rc' && !sameBase.some((item) => item.channel === 'dev' || item.channel === 'rc')) {
    fail(`rc ${published.baseVersion} requires an existing dev release`);
  }
  if (published.channel === 'stable' && !sameBase.some((item) => item.channel === 'rc')) {
    fail(`stable ${published.baseVersion} requires an existing rc release`);
  }
  return published;
}

export function cleanupTagsForPublishedTag(publishedTag, tags) {
  const published = parseManagedTag(publishedTag);
  if (!published) fail(`published tag '${publishedTag}' is not managed by the release policy`);
  if (published.channel === 'dev') return [];

  const candidates = [...new Set(tags)]
    .map(parseManagedTag)
    .filter(Boolean)
    .filter((item) => item.baseVersion === published.baseVersion)
    .filter((item) => item.tag !== published.tag)
    .filter((item) => item.channel !== 'stable')
    .filter((item) => published.channel === 'stable' || item.channel === 'dev' || item.channel === 'rc')
    .map((item) => item.tag)
    .sort();

  if (candidates.some((tag) => parseManagedTag(tag)?.channel === 'stable')) {
    fail('internal error: stable tag selected for cleanup');
  }
  return candidates;
}

export function selectPreviousReleaseTag(currentTag, tags) {
  const current = parseManagedTag(currentTag);
  if (!current) return null;
  const parsed = [...new Set(tags)]
    .map(parseManagedTag)
    .filter(Boolean)
    .filter((item) => item.tag !== current.tag);

  const sameBase = parsed.filter((item) => item.baseVersion === current.baseVersion);
  if (current.channel === 'stable') {
    const rc = sameBase.filter((item) => item.channel === 'rc');
    return rc.sort((a, b) => compareBuildIds(a.buildId, b.buildId)).at(-1)?.tag ?? null;
  }
  if (current.channel === 'rc') {
    const rc = sameBase.filter((item) => item.channel === 'rc');
    const previousRc = rc.sort((a, b) => compareBuildIds(a.buildId, b.buildId)).at(-1);
    if (previousRc) return previousRc.tag;
    const stable = parsed.filter((item) => item.channel === 'stable');
    return stable.sort((a, b) => compareBaseVersions(a.baseVersion, b.baseVersion)).at(-1)?.tag ?? null;
  }

  const dev = sameBase.filter((item) => item.channel === 'dev');
  const previousDev = dev.sort((a, b) => compareBuildIds(a.buildId, b.buildId)).at(-1);
  if (previousDev) return previousDev.tag;
  const stable = parsed.filter((item) => item.channel === 'stable');
  return stable.sort((a, b) => compareBaseVersions(a.baseVersion, b.baseVersion)).at(-1)?.tag ?? null;
}

function git(args, options = {}) {
  const output = execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options });
  return output == null ? '' : output.trim();
}

function livePlan(input) {
  const branch = git(['symbolic-ref', '--quiet', '--short', 'HEAD']);
  if (!['develop', 'main'].includes(branch)) {
    fail(`releases are only allowed from develop or main; current branch is '${branch}'`);
  }
  const remotes = git(['remote']).split(/\r?\n/).filter(Boolean);
  if (!remotes.includes('origin')) fail("release authority remote 'origin' is required");

  git(['fetch', '--prune', 'origin', `+refs/heads/${branch}:refs/remotes/origin/${branch}`], { stdio: ['ignore', 'ignore', 'pipe'] });
  const head = git(['rev-parse', 'HEAD']);
  const authorityHead = git(['rev-parse', `refs/remotes/origin/${branch}`]);
  if (head !== authorityHead) {
    fail(`${branch} must exactly match origin/${branch} before releasing`);
  }

  const remoteTags = git(['ls-remote', '--tags', '--refs', 'origin'])
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => line.split(/\s+/)[1]?.replace(/^refs\/tags\//, ''))
    .filter(Boolean);
  const buildNumber = Number(git(['rev-list', '--count', 'HEAD'])) + 1;
  return resolveReleasePlan({ branch, input, tags: remoteTags, buildNumber });
}

function usage() {
  console.log(`Usage:\n  node scripts/release-policy.mjs plan <X.Y.Z|X.Y.Z-rc>\n  node scripts/release-policy.mjs validate-published <branch> <tag> [tag ...]\n  node scripts/release-policy.mjs cleanup <published-tag> [tag ...]\n  node scripts/release-policy.mjs previous <current-tag> [tag ...]`);
}

function runCli() {
  const [command, value, ...rest] = process.argv.slice(2);
  if (command === 'plan' && value && rest.length === 0) {
    console.log(JSON.stringify(livePlan(value)));
    return;
  }
  if (command === 'validate-published' && value && rest.length >= 1) {
    const [tag, ...tags] = rest;
    validatePublishedRelease({ branch: value, tag, tags });
    console.log(tag);
    return;
  }
  if (command === 'cleanup' && value) {
    console.log(cleanupTagsForPublishedTag(value, rest).join('\n'));
    return;
  }
  if (command === 'previous' && value) {
    console.log(selectPreviousReleaseTag(value, rest) ?? '');
    return;
  }
  usage();
  process.exitCode = 2;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    runCli();
  } catch (error) {
    console.error(`ERROR: ${error.message}`);
    process.exitCode = 1;
  }
}
