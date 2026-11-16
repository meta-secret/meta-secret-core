import { readFileSync, lstatSync, realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { androidProfileHash, androidProfileIni, assertAndroidOwnedPath } from './android-owned-profile.mjs';

function validateProfile(profile, { runId, avdName }) {
  if (profile?.version !== 1 || profile.runId !== runId || profile.avdName !== avdName
      || avdName !== 'Pixel_4a' || !/^MetaSecret-e2e-[0-9a-f-]{36}$/.test(profile.physicalAvdName ?? '')) {
    throw new Error('Owned Android handoff scope mismatch');
  }
  const stat = lstatSync(profile.root);
  if (!stat.isDirectory() || stat.isSymbolicLink() || realpathSync(profile.root) !== profile.root
      || stat.dev !== profile.identity?.dev || stat.ino !== profile.identity?.ino) throw new Error('Owned Android root changed');
  if (profile.content !== resolve(profile.root, 'content') || profile.avdHome !== resolve(profile.root, 'user/.android/avd')
      || profile.configPath !== resolve(profile.content, 'config.ini')) throw new Error('Owned Android paths mismatch');
  assertAndroidOwnedPath(profile.root, profile.configPath);
  if (androidProfileHash(readFileSync(profile.configPath)) !== profile.configSha256) throw new Error('Owned Android config changed');
  const pointer = resolve(profile.avdHome, `${profile.physicalAvdName}.ini`);
  assertAndroidOwnedPath(profile.root, pointer);
  if (androidProfileIni(readFileSync(pointer, 'utf8')).path !== profile.content) throw new Error('Owned Android selector changed');
  const expected = { ANDROID_AVD_HOME: profile.avdHome, ANDROID_USER_HOME: resolve(profile.root, 'user/.android'),
    ANDROID_EMULATOR_HOME: resolve(profile.root, 'user/.android'), ANDROID_SDK_HOME: resolve(profile.root, 'user'),
    ANDROID_HOME: profile.sdkRoot, ANDROID_SDK_ROOT: profile.sdkRoot };
  for (const [key, value] of Object.entries(expected)) {
    if (profile.privateEnv?.[key] !== value) throw new Error('Owned Android private environment mismatch');
  }
  return profile;
}

export function readOwnedAndroidHandoff(avdName, { env = process.env, runId = env.E2E_RUN_ID,
  readinessPath = env.E2E_READINESS_PATH, required = env.E2E_ANDROID_OWNED_PROFILE !== undefined } = {}) {
  if (!required) return undefined;
  if (env.E2E_ANDROID_OWNED_PROFILE !== '1' || env.E2E_RANGE_RUN !== '1' || !readinessPath || !runId) {
    throw new Error('Declared owned Android handoff is unavailable');
  }
  const report = JSON.parse(readFileSync(readinessPath, 'utf8'));
  if (report.runId !== runId || env.E2E_TEST_NUMBER && report.test !== Number(env.E2E_TEST_NUMBER)
      || env.E2E_TEST_ATTEMPT && report.attempt !== Number(env.E2E_TEST_ATTEMPT)) throw new Error('Owned Android readiness scope mismatch');
  const matches = report.platforms?.filter(item => item.platform === 'Android') ?? [];
  if (matches.length !== 1 || matches[0].avdName !== avdName || matches[0].state !== 'Booted'
      || !/^emulator-\d+$/.test(matches[0].serial) || !Number.isSafeInteger(matches[0].emulatorPid)
      || matches[0].emulatorPid <= 0) throw new Error('Owned Android readiness is absent or ambiguous');
  validateProfile(matches[0].ownedAndroidProfile, { runId, avdName });
  return matches[0];
}

export function resolveAndroidScenarioAvdName(avdName, options) {
  return readOwnedAndroidHandoff(avdName, options)?.ownedAndroidProfile.physicalAvdName ?? avdName;
}

function requireRuntimeFile(profile, path) {
  assertAndroidOwnedPath(profile.root, path);
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || realpathSync(path) !== path) throw new Error('Owned Android runtime file is not regular and canonical');
  return stat;
}

function verifyRuntimeArguments(profile, tokens, hardware) {
  const executable = resolve(profile.sdkRoot, 'emulator/qemu/darwin-aarch64/qemu-system-aarch64');
  if (tokens[0] !== executable || !lstatSync(executable).isFile() || realpathSync(executable) !== executable) {
    throw new Error('Owned Android QEMU executable differs from installed SDK');
  }
  const expected = ['-avd', profile.physicalAvdName, '-datadir', profile.content,
    '-data', resolve(profile.content, 'userdata-qemu.img'), '-cache', resolve(profile.content, 'cache.img'),
    '-no-snapshot', '-no-snapstorage'];
  if (!tokens.includes('-android-hw')) {
    if (JSON.stringify(tokens.slice(1)) !== JSON.stringify(expected)) throw new Error('Owned Android frontend launch vector differs');
    return;
  }
  // Retain the legacy representation, without accepting an invalid present alias as frontend fallback.
  const pairs = { '-avd': profile.physicalAvdName, '-android-hw': hardware, '-datadir': profile.content,
    '-data': resolve(profile.content, 'userdata-qemu.img'), '-cache': resolve(profile.content, 'cache.img') };
  const seen = new Set();
  for (let index = 1; index < tokens.length; index += 1) {
    const flag = tokens[index];
    if (seen.has(flag)) throw new Error('Owned Android launch flag is ambiguous');
    seen.add(flag);
    if (Object.hasOwn(pairs, flag)) {
      if (tokens[++index] !== pairs[flag]) throw new Error('Owned Android legacy path differs');
    } else if (!['-no-snapshot', '-no-snapstorage'].includes(flag)) throw new Error('Owned Android legacy flag is unclassified');
  }
  if (!seen.has('-avd') || !seen.has('-android-hw')) throw new Error('Owned Android legacy identity is missing');
}

function verifyRuntimeMetadata(profile, recorded) {
  const identity = { 'avd.id': profile.physicalAvdName, 'avd.name': profile.physicalAvdName,
    'android.avd.home': profile.avdHome, 'android.sdk.root': profile.sdkRoot };
  for (const [key, value] of Object.entries(identity)) {
    if (recorded[key] !== value) throw new Error(`Owned Android hardware identity differs: ${key}`);
  }
  for (const [key, name] of Object.entries({ 'disk.dataPartition.path': 'userdata-qemu.img',
    'disk.cachePartition.path': 'cache.img', 'disk.encryptionKeyPartition.path': 'encryptionkey.img' })) {
    const expected = resolve(profile.content, name);
    if (recorded[key] !== expected) throw new Error(`Owned Android writable path differs: ${key}`);
    requireRuntimeFile(profile, expected);
  }
  const sd = resolve(profile.content, 'sdcard.img');
  if (requireRuntimeFile(profile, sd).size !== 512 * 1024 ** 2) throw new Error('Owned Android SD image size differs');
  if (recorded['hw.sdCard.path'] !== undefined) {
    if (recorded['hw.sdCard.path'] !== sd) throw new Error('Owned Android SD path differs');
    return;
  }
  // The current ARM API36 package does not auto-attach SD (AOSP BUG174481551); this is not a general missing-path allowance.
  const config = androidProfileIni(readFileSync(profile.configPath, 'utf8'));
  if (config['abi.type'] !== 'arm64-v8a' || config['hw.cpu.arch'] !== 'arm64' || config.target !== 'android-36'
      || !config['image.sysdir.1'] || resolve(profile.sdkRoot, config['image.sysdir.1']) !== resolve(profile.sdkRoot, 'system-images/android-36/google_apis_ps16k/arm64-v8a')
      || config['hw.sdCard'] !== 'yes' || config['sdcard.size'] !== '512M'
      || recorded['hw.cpu.arch'] !== 'arm64' || recorded['hw.sdCard'] !== 'true') {
    throw new Error('Owned Android automatic SD omission is unqualified');
  }
}

export async function verifyOwnedAndroidRuntime(platform, { runId = process.env.E2E_RUN_ID, runCommand }) {
  const profile = validateProfile(platform.ownedAndroidProfile, { runId, avdName: platform.avdName });
  if (typeof runCommand !== 'function' || !Number.isSafeInteger(platform.emulatorPid) || platform.emulatorPid <= 0) {
    throw new Error('Owned Android runtime process proof missing');
  }
  const args = process.platform === 'darwin' ? ['-axo', 'pid=,command='] : ['-eo', 'pid=,args='];
  const output = await runCommand('ps', args, { timeout: 10_000 });
  const matches = [];
  for (const line of output.stdout.split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(.+)$/);
    if (!match) continue;
    const tokens = match[2].match(/"[^"]*"|'[^']*'|\S+/g)?.map(token => token.replace(/^['"]|['"]$/g, '')) ?? [];
    if (!/^qemu-system(?:-|$)/.test(tokens[0]?.split('/').pop() ?? '')) continue;
    if (tokens[tokens.indexOf('-avd') + 1] !== profile.physicalAvdName) continue;
    matches.push({ pid: Number(match[1]), tokens });
  }
  if (matches.length !== 1 || matches[0].pid !== platform.emulatorPid) throw new Error('Owned Android QEMU is missing or ambiguous');
  const hardware = resolve(profile.content, 'hardware-qemu.ini');
  verifyRuntimeArguments(profile, matches[0].tokens, hardware);
  if (requireRuntimeFile(profile, hardware).size > 65536) throw new Error('Owned Android hardware metadata exceeded bound');
  const recorded = androidProfileIni(readFileSync(hardware, 'utf8'));
  verifyRuntimeMetadata(profile, recorded);
  return platform;
}

export async function verifyOwnedAndroidScenarioRuntime(avdName, serial, { env = process.env, runCommand } = {}) {
  const platform = readOwnedAndroidHandoff(avdName, { env });
  if (!platform) return;
  if (!serial || platform.serial !== serial) throw new Error('Owned Android serial handoff lost; legacy launch forbidden');
  await verifyOwnedAndroidRuntime(platform, { runId: env.E2E_RUN_ID, runCommand });
  return platform;
}

export async function verifyOwnedAndroidResetTarget(platform, { runId, readinessPath, runCommand }) {
  const expected = readOwnedAndroidHandoff(platform.avdName, { runId, readinessPath, required: true,
    env: { E2E_ANDROID_OWNED_PROFILE: '1', E2E_RANGE_RUN: '1' } });
  if (platform.serial !== expected.serial || JSON.stringify(platform.ownedAndroidProfile) !== JSON.stringify(expected.ownedAndroidProfile)
      || platform.emulatorPid !== expected.emulatorPid) throw new Error('Owned Android reset target differs from readiness');
  return verifyOwnedAndroidRuntime(expected, { runId, runCommand });
}
