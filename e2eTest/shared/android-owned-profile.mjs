import { createHash, randomUUID } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { isAbsolute, relative, resolve } from 'node:path';

export const OWNED_ANDROID_SPEC = Object.freeze({
  logicalAvdName: 'Pixel_4a', device: 'pixel_4a', abi: 'arm64-v8a',
  package: 'system-images;android-36;google_apis_ps16k;arm64-v8a',
  image: 'system-images/android-36/google_apis_ps16k/arm64-v8a',
  packageSha256: 'bfb35a9036ef5fdd71ba5ab6247031ffef1131f20d3c635931ad1c4f39dfe7e5',
  fixtureSha256: '5958228dadea419fbc98495c3bdf7da0efe30ec041c0bf15b49ffff1ed0879c2',
});
const guards = Object.freeze({ 'abi.type': 'arm64-v8a', 'hw.cpu.arch': 'arm64',
  'hw.device.name': 'pixel_4a', 'hw.device.hash2': 'MD5:7250072fa23402acb695d3eb0f172641',
  'tag.id': 'page_size_16kb', 'tag.ids': 'page_size_16kb,google_apis' });
const normalized = Object.freeze({ 'hw.lcd.width': '1080', 'hw.lcd.height': '2340', 'hw.lcd.density': '440',
  'hw.ramSize': '2048', 'hw.cpu.ncore': '4', 'hw.keyboard': 'yes', 'hw.gpu.enabled': 'yes', 'hw.gpu.mode': 'auto',
  'disk.dataPartition.size': '6442450944', 'hw.sdCard': 'yes', 'sdcard.size': '512M', 'skin.name': 'pixel_4a',
  'PlayStore.enabled': 'false', 'hw.camera.back': 'virtualscene', 'hw.camera.front': 'emulated',
  'hw.initialOrientation': 'portrait' });
// Fixed observed CLI19/default-only contract; source schema SHA7e4290f6, never a dynamic allow-all rule.
const defaultOnly = Object.freeze({
  'disk.cachePartition': 'yes', 'disk.cachePartition.size': '66MB',
  'disk.systemPartition.size': '0', 'disk.vendorPartition.size': '0',
  'firstboot.bootFromDownloadableSnapshot': 'yes', 'firstboot.bootFromLocalSnapshot': 'yes',
  'firstboot.saveToLocalSnapshot': 'yes', 'hw.accelerometer_uncalibrated': 'yes',
  'hw.arc.autologin': 'no', 'hw.audioOutput': 'yes',
  'hw.camera.back.orientation': '90', 'hw.camera.front.orientation': '90',
  'hw.display1.density': '0', 'hw.display1.flag': '0',
  'hw.display1.height': '0', 'hw.display1.width': '0',
  'hw.display1.xOffset': '-1', 'hw.display1.yOffset': '-1',
  'hw.display2.density': '0', 'hw.display2.flag': '0',
  'hw.display2.height': '0', 'hw.display2.width': '0',
  'hw.display2.xOffset': '-1', 'hw.display2.yOffset': '-1',
  'hw.display3.density': '0', 'hw.display3.flag': '0',
  'hw.display3.height': '0', 'hw.display3.width': '0',
  'hw.display3.xOffset': '-1', 'hw.display3.yOffset': '-1',
  'hw.displayRegion.0.1.height': '0', 'hw.displayRegion.0.1.width': '0',
  'hw.displayRegion.0.1.xOffset': '-1', 'hw.displayRegion.0.1.yOffset': '-1',
  'hw.displayRegion.0.2.height': '0', 'hw.displayRegion.0.2.width': '0',
  'hw.displayRegion.0.2.xOffset': '-1', 'hw.displayRegion.0.2.yOffset': '-1',
  'hw.displayRegion.0.3.height': '0', 'hw.displayRegion.0.3.width': '0',
  'hw.displayRegion.0.3.xOffset': '-1', 'hw.displayRegion.0.3.yOffset': '-1',
  'hw.gltransport': 'pipe', 'hw.gltransport.asg.dataRingSize': '32768',
  'hw.gltransport.asg.writeBufferSize': '1048576', 'hw.gltransport.asg.writeStepSize': '4096',
  'hw.gltransport.drawFlushInterval': '800', 'hw.gsmModem': 'yes',
  'hw.hotplug_multi_display': 'no', 'hw.keyboard.charmap': 'qwerty2',
  'hw.keyboard.lid': 'yes', 'hw.lcd.backlight': 'yes',
  'hw.lcd.circular': 'false', 'hw.lcd.depth': '16',
  'hw.lcd.transparent': 'false', 'hw.lcd.vsync': '60',
  'hw.multi_display_window': 'no', 'hw.rotaryInput': 'no',
  'hw.screen': 'multi-touch', 'hw.sensor.hinge': 'no',
  'hw.sensor.hinge.count': '0', 'hw.sensor.hinge.fold_to_displayRegion.0.1_at_posture': '1',
  'hw.sensor.hinge.resizable.config': '1', 'hw.sensor.hinge.sub_type': '0',
  'hw.sensor.hinge.type': '0', 'hw.sensor.roll': 'no',
  'hw.sensor.roll.count': '0', 'hw.sensor.roll.resize_to_displayRegion.0.1_at_posture': '6',
  'hw.sensor.roll.resize_to_displayRegion.0.2_at_posture': '6', 'hw.sensor.roll.resize_to_displayRegion.0.3_at_posture': '6',
  'hw.sensors.gyroscope_uncalibrated': 'yes', 'hw.sensors.heading': 'no',
  'hw.sensors.heart_rate': 'no', 'hw.sensors.humidity': 'yes',
  'hw.sensors.magnetic_field_uncalibrated': 'yes', 'hw.sensors.rgbclight': 'no',
  'hw.sensors.temperature': 'yes', 'hw.sensors.wrist_tilt': 'no',
  'hw.touchpad0': 'no', 'hw.touchpad0.height': '400',
  'hw.touchpad0.width': '600', 'hw.useext4': 'yes',
  'kernel.newDeviceNaming': 'autodetect', 'kernel.supportsYaffs2': 'autodetect',
  'test.delayAdbTillBootComplete': '0', 'test.monitorAdb': '0',
  'test.quitAfterBootTimeOut': '-1', 'userdata.useQcow2': 'no',
});
const namespaces = new Set(['AvdId', 'avd.ini.displayname', 'avd.id', 'avd.name']);
const imagePaths = new Set(['sdcard.path', 'disk.cachePartition.path', 'disk.dataPartition.path',
  'disk.encryptionKeyPartition.path']);
export const androidProfileHash = bytes => createHash('sha256').update(bytes).digest('hex');

export function androidProfileIni(text) {
  const values = {};
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim() || /^\s*[#;]/.test(line)) continue;
    const match = line.match(/^\s*([^=\s]+)\s*=\s*(.*)$/);
    if (!match || Object.hasOwn(values, match[1])) throw new Error('Ambiguous Android profile configuration');
    values[match[1]] = match[2].trim();
  }
  return values;
}

export function assertAndroidOwnedPath(root, path) {
  const suffix = relative(root, path);
  if (!suffix || suffix.startsWith('..') || isAbsolute(suffix)) throw new Error('Android profile path escaped its owned root');
  let current = root;
  for (const part of suffix.split('/')) {
    current = resolve(current, part);
    if (existsSync(current) && lstatSync(current).isSymbolicLink()) throw new Error('Android profile symlink refused');
  }
  return path;
}

function regular(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('Required Android profile input is not a regular file');
  return readFileSync(path);
}

function checkRoot(root, identity) {
  const stat = lstatSync(root);
  if (!stat.isDirectory() || stat.isSymbolicLink() || realpathSync(root) !== root
      || stat.dev !== identity.dev || stat.ino !== identity.ino) throw new Error('Android owned root identity changed');
}

function bindNewNames(values, physicalName) {
  const legacy = ['AvdId', 'avd.ini.displayname'], hardware = ['avd.id', 'avd.name'];
  const canonical = legacy.every(key => values[key] === physicalName) && hardware.every(key => values[key] === undefined);
  const template = legacy.every(key => values[key] === undefined) && hardware.every(key => values[key] === '<build>');
  if (!canonical && !template) throw new Error('New Android physical name mismatch');
  for (const key of namespaces) values[key] = physicalName;
}

function normalizeObservedMetadata(values, baseline) {
  for (const [key, observed] of Object.entries({ 'hw.arc': 'no', 'vm.heapSize': '256M' })) {
    if (!Object.hasOwn(baseline, key)) continue;
    if (values[key] !== observed && values[key] !== baseline[key]) throw new Error(`Unclassified Android representation: ${key}`);
    values[key] = baseline[key];
  }
  for (const [key, expected] of Object.entries({ 'fastboot.chosenSnapshotFile': '', 'skin.dynamic': 'yes', target: 'android-36' })) {
    if (!Object.hasOwn(baseline, key)) continue;
    if (baseline[key] !== expected || values[key] !== undefined && values[key] !== expected) throw new Error(`Unclassified Android metadata: ${key}`);
    values[key] = expected;
  }
}

function bindOwnedData(values, scope) {
  const key = 'disk.dataPartition.path', expected = resolve(scope.content, 'userdata-qemu.img');
  if (values[key] !== undefined && values[key] !== '<temp>' && values[key] !== expected) throw new Error(`Android data path mismatch: ${key}`);
  assertAndroidOwnedPath(scope.root, expected); values[key] = expected;
}

function dropKnownDefaults(values) {
  for (const [key, expected] of Object.entries(defaultOnly)) {
    if (!Object.hasOwn(values, key)) continue;
    if (values[key] !== expected) throw new Error(`Unclassified Android default difference: ${key}`);
    // Reference omissions preserve API-specific defaults, which may differ from the literal schema default.
    delete values[key];
  }
}

function normalizeNewConfig(configPath, baseline, scope) {
  const values = androidProfileIni(regular(configPath).toString());
  for (const [key, value] of Object.entries(guards)) {
    if (values[key] !== value || baseline[key] !== value) throw new Error(`Android fixture identity mismatch: ${key}`);
  }
  bindNewNames(values, scope.physicalAvdName); normalizeObservedMetadata(values, baseline);
  bindOwnedData(values, scope); dropKnownDefaults(values);
  const permitted = { ...normalized, 'skin.path': resolve(scope.sdkRoot, 'skins/pixel_4a') };
  for (const [key, value] of Object.entries(permitted)) {
    if (baseline[key] !== value) throw new Error(`Pinned Android fixture changed: ${key}`);
    values[key] = value;
  }
  for (const key of new Set([...Object.keys(baseline), ...Object.keys(values)])) {
    if (Object.hasOwn(permitted, key) || namespaces.has(key)) continue;
    if (imagePaths.has(key) && values[key]) { assertAndroidOwnedPath(scope.content, resolve(values[key])); continue; }
    if (key === 'image.sysdir.1') {
      if (resolve(scope.sdkRoot, values[key] ?? '') !== scope.image) throw new Error('Android system image mismatch');
      continue;
    }
    if (baseline[key] !== values[key]) throw new Error(`Unclassified Android default difference: ${key}`);
  }
  writeFileSync(configPath, Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join(''), { mode: 0o600 });
  const after = androidProfileIni(regular(configPath).toString());
  if (JSON.stringify(after) !== JSON.stringify(values)) throw new Error('Android profile normalization reread failed');
  return androidProfileHash(regular(configPath));
}

/** Scope is current-process ownership, not a persisted JSON adoption authority. */
export async function createRunOwnedAndroidProfile({ runId, avdName, sdk, runCommand, onIntent = () => {},
  referenceConfig = resolve(homedir(), '.android/avd/Pixel_4a.avd/config.ini'), javaHome,
  expectedFixtureHash = OWNED_ANDROID_SPEC.fixtureSha256, expectedPackageHash = OWNED_ANDROID_SPEC.packageSha256,
  privateParent = tmpdir() }) {
  if (typeof runId !== 'string' || !runId || avdName !== OWNED_ANDROID_SPEC.logicalAvdName || typeof runCommand !== 'function') {
    throw new Error('Explicit supported Android owned-profile scope required');
  }
  const baselineBytes = regular(referenceConfig);
  const image = resolve(sdk.sdkRoot, OWNED_ANDROID_SPEC.image);
  if (androidProfileHash(baselineBytes) !== expectedFixtureHash
      || androidProfileHash(regular(resolve(image, 'package.xml'))) !== expectedPackageHash) {
    throw new Error('Installed Android profile inputs changed');
  }
  const packageXml = regular(resolve(image, 'package.xml')).toString();
  if (!packageXml.includes(`path="${OWNED_ANDROID_SPEC.package}"`)) throw new Error('Android package ID mismatch');
  for (const path of [sdk.emulatorPath, resolve(sdk.sdkRoot, 'cmdline-tools/latest/bin/avdmanager'),
    resolve(sdk.sdkRoot, 'emulator/mksdcard'), resolve(image, 'data/empty_data_disk'), resolve(image, 'encryptionkey.img')]) regular(path);
  javaHome ??= (await runCommand('/usr/libexec/java_home', ['-v', '21'], { timeout: 10_000 })).stdout.trim();
  if (!/^JAVA_VERSION="21(?:[."])/m.test(regular(resolve(javaHome, 'release')).toString())) throw new Error('JDK21 is required');
  regular(resolve(javaHome, 'bin/java'));
  const root = realpathSync(mkdtempSync(resolve(privateParent, 'metasecret-owned-avd-')));
  const stat = lstatSync(root), identity = { dev: stat.dev, ino: stat.ino };
  const content = resolve(root, 'content'), androidHome = resolve(root, 'user/.android'), avdHome = resolve(androidHome, 'avd');
  const physicalAvdName = `MetaSecret-e2e-${randomUUID()}`;
  const privateEnv = { JAVA_HOME: javaHome, ANDROID_HOME: sdk.sdkRoot, ANDROID_SDK_ROOT: sdk.sdkRoot,
    ANDROID_USER_HOME: androidHome, ANDROID_AVD_HOME: avdHome, ANDROID_EMULATOR_HOME: androidHome,
    ANDROID_SDK_HOME: resolve(root, 'user'), XDG_CACHE_HOME: resolve(root, 'cache'), JAVA_OPTS: '',
    JAVA_TOOL_OPTIONS: `-Duser.home=${resolve(root, 'user')} -Djava.util.prefs.userRoot=${resolve(root, 'prefs')}` };
  const intent = Object.freeze({ version: 1, runId, avdName, physicalAvdName, root, identity, content, avdHome, privateEnv });
  let writing = false, removed = false, profile;
  const lease = { intent,
    get removed() { return removed; },
    verify(scope = {}) {
      if (removed || writing || !profile || scope.runId && scope.runId !== runId || scope.avdName && scope.avdName !== avdName) {
        throw new Error('Android profile lease is missing, foreign or busy');
      }
      checkRoot(root, identity);
      if (androidProfileHash(regular(profile.configPath)) !== profile.configSha256) throw new Error('Android owned configuration changed');
      return profile;
    },
    remove() {
      if (removed) return;
      if (writing) throw new Error('Android creator is not closed; retaining private root');
      checkRoot(root, identity);
      rmSync(root, { recursive: true }); removed = true;
    },
    toJSON() { return profile ?? intent; },
  };
  try {
    onIntent(lease);
    for (const directory of [avdHome, privateEnv.XDG_CACHE_HOME, resolve(root, 'prefs')]) mkdirSync(directory, { recursive: true, mode: 0o700 });
    const args = ['create', 'avd', '-n', physicalAvdName, '-k', OWNED_ANDROID_SPEC.package,
      '-d', OWNED_ANDROID_SPEC.device, '-b', OWNED_ANDROID_SPEC.abi, '-c', '512M', '-p', content];
    // Caller supplies the existing preparation-command group/close barrier. No force, shell or old data copy.
    writing = true;
    try { await runCommand(resolve(sdk.sdkRoot, 'cmdline-tools/latest/bin/avdmanager'), args, { env: privateEnv, timeout: 120_000 }); }
    finally { writing = false; }
    const selector = resolve(avdHome, `${physicalAvdName}.ini`), configPath = resolve(content, 'config.ini');
    if (androidProfileIni(regular(selector).toString()).path !== content) throw new Error('Android private selector mismatch');
    assertAndroidOwnedPath(root, selector); assertAndroidOwnedPath(root, configPath);
    const configSha256 = normalizeNewConfig(configPath, androidProfileIni(baselineBytes.toString()), { ...intent, sdkRoot: sdk.sdkRoot, image });
    const sdcard = assertAndroidOwnedPath(root, resolve(content, 'sdcard.img')), sdcardStat = lstatSync(sdcard);
    if (!sdcardStat.isFile() || sdcardStat.isSymbolicLink() || sdcardStat.size !== 512 * 1024 ** 2) throw new Error('New Android SD image is not a regular owned 512M image');
    profile = Object.freeze({ ...intent, configPath, configSha256, sdkRoot: sdk.sdkRoot,
      launchArgs: ['-avd', physicalAvdName, '-datadir', content, '-data', resolve(content, 'userdata-qemu.img'),
        '-cache', resolve(content, 'cache.img'), '-no-snapshot', '-no-snapstorage'] });
    lease.verify(); return lease;
  } catch (error) {
    try { lease.remove(); } catch (cleanupError) {
      throw new AggregateError([error, cleanupError], 'Android profile creation failed; cleanup incomplete', { cause: error });
    }
    throw error;
  }
}
