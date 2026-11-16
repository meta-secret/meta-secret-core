import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { createRunOwnedAndroidProfile, androidProfileHash, androidProfileIni, OWNED_ANDROID_SPEC } from './android-owned-profile.mjs';

// Independent literal SDK04 capture138 and reference54; only skin.path relocates into the fake SDK.
const SDK04_GENERATED = Object.freeze({
  "PlayStore.enabled": "no", "abi.type": "arm64-v8a",
  "avd.id": "<build>", "avd.ini.encoding": "UTF-8",
  "avd.name": "<build>", "disk.cachePartition": "yes",
  "disk.cachePartition.size": "66MB", "disk.dataPartition.path": "<temp>",
  "disk.dataPartition.size": "2G", "disk.systemPartition.size": "0",
  "disk.vendorPartition.size": "0", "fastboot.forceChosenSnapshotBoot": "no",
  "fastboot.forceColdBoot": "no", "fastboot.forceFastBoot": "yes",
  "firstboot.bootFromDownloadableSnapshot": "yes", "firstboot.bootFromLocalSnapshot": "yes",
  "firstboot.saveToLocalSnapshot": "yes", "hw.accelerometer": "yes",
  "hw.accelerometer_uncalibrated": "yes", "hw.arc": "no",
  "hw.arc.autologin": "no", "hw.audioInput": "yes",
  "hw.audioOutput": "yes", "hw.battery": "yes",
  "hw.camera.back": "emulated", "hw.camera.back.orientation": "90",
  "hw.camera.front": "none", "hw.camera.front.orientation": "90",
  "hw.cpu.arch": "arm64", "hw.cpu.ncore": "4",
  "hw.dPad": "no", "hw.device.hash2": "MD5:7250072fa23402acb695d3eb0f172641",
  "hw.device.manufacturer": "Google", "hw.device.name": "pixel_4a",
  "hw.display1.density": "0", "hw.display1.flag": "0",
  "hw.display1.height": "0", "hw.display1.width": "0",
  "hw.display1.xOffset": "-1", "hw.display1.yOffset": "-1",
  "hw.display2.density": "0", "hw.display2.flag": "0",
  "hw.display2.height": "0", "hw.display2.width": "0",
  "hw.display2.xOffset": "-1", "hw.display2.yOffset": "-1",
  "hw.display3.density": "0", "hw.display3.flag": "0",
  "hw.display3.height": "0", "hw.display3.width": "0",
  "hw.display3.xOffset": "-1", "hw.display3.yOffset": "-1",
  "hw.displayRegion.0.1.height": "0", "hw.displayRegion.0.1.width": "0",
  "hw.displayRegion.0.1.xOffset": "-1", "hw.displayRegion.0.1.yOffset": "-1",
  "hw.displayRegion.0.2.height": "0", "hw.displayRegion.0.2.width": "0",
  "hw.displayRegion.0.2.xOffset": "-1", "hw.displayRegion.0.2.yOffset": "-1",
  "hw.displayRegion.0.3.height": "0", "hw.displayRegion.0.3.width": "0",
  "hw.displayRegion.0.3.xOffset": "-1", "hw.displayRegion.0.3.yOffset": "-1",
  "hw.gltransport": "pipe", "hw.gltransport.asg.dataRingSize": "32768",
  "hw.gltransport.asg.writeBufferSize": "1048576", "hw.gltransport.asg.writeStepSize": "4096",
  "hw.gltransport.drawFlushInterval": "800", "hw.gps": "yes",
  "hw.gpu.enabled": "no", "hw.gpu.mode": "auto",
  "hw.gsmModem": "yes", "hw.gyroscope": "yes",
  "hw.hotplug_multi_display": "no", "hw.initialOrientation": "portrait",
  "hw.keyboard": "no", "hw.keyboard.charmap": "qwerty2",
  "hw.keyboard.lid": "yes", "hw.lcd.backlight": "yes",
  "hw.lcd.circular": "false", "hw.lcd.density": "440",
  "hw.lcd.depth": "16", "hw.lcd.height": "2340",
  "hw.lcd.transparent": "false", "hw.lcd.vsync": "60",
  "hw.lcd.width": "1080", "hw.mainKeys": "no",
  "hw.multi_display_window": "no", "hw.ramSize": "2G",
  "hw.rotaryInput": "no", "hw.screen": "multi-touch",
  "hw.sdCard": "yes", "hw.sensor.hinge": "no",
  "hw.sensor.hinge.count": "0", "hw.sensor.hinge.fold_to_displayRegion.0.1_at_posture": "1",
  "hw.sensor.hinge.resizable.config": "1", "hw.sensor.hinge.sub_type": "0",
  "hw.sensor.hinge.type": "0", "hw.sensor.roll": "no",
  "hw.sensor.roll.count": "0", "hw.sensor.roll.resize_to_displayRegion.0.1_at_posture": "6",
  "hw.sensor.roll.resize_to_displayRegion.0.2_at_posture": "6", "hw.sensor.roll.resize_to_displayRegion.0.3_at_posture": "6",
  "hw.sensors.gyroscope_uncalibrated": "yes", "hw.sensors.heading": "no",
  "hw.sensors.heart_rate": "no", "hw.sensors.humidity": "yes",
  "hw.sensors.light": "yes", "hw.sensors.magnetic_field": "yes",
  "hw.sensors.magnetic_field_uncalibrated": "yes", "hw.sensors.orientation": "yes",
  "hw.sensors.pressure": "yes", "hw.sensors.proximity": "yes",
  "hw.sensors.rgbclight": "no", "hw.sensors.temperature": "yes",
  "hw.sensors.wrist_tilt": "no", "hw.touchpad0": "no",
  "hw.touchpad0.height": "400", "hw.touchpad0.width": "600",
  "hw.trackBall": "no", "hw.useext4": "yes",
  "image.sysdir.1": "system-images/android-36/google_apis_ps16k/arm64-v8a/", "kernel.newDeviceNaming": "autodetect",
  "kernel.supportsYaffs2": "autodetect", "runtime.network.latency": "none",
  "runtime.network.speed": "full", "sdcard.size": "512M",
  "showDeviceFrame": "yes", "tag.display": "16 KB Page Size",
  "tag.displaynames": "16 KB Page Size,Google APIs", "tag.id": "page_size_16kb",
  "tag.ids": "page_size_16kb,google_apis", "test.delayAdbTillBootComplete": "0",
  "test.monitorAdb": "0", "test.quitAfterBootTimeOut": "-1",
  "userdata.useQcow2": "no", "vm.heapSize": "256M",
});

const SDK04_REFERENCE = Object.freeze({
  "AvdId": "Pixel_4a", "PlayStore.enabled": "false",
  "abi.type": "arm64-v8a", "avd.ini.displayname": "Pixel 4a",
  "avd.ini.encoding": "UTF-8", "disk.dataPartition.size": "6442450944",
  "fastboot.chosenSnapshotFile": "", "fastboot.forceChosenSnapshotBoot": "no",
  "fastboot.forceColdBoot": "no", "fastboot.forceFastBoot": "yes",
  "hw.accelerometer": "yes", "hw.arc": "false",
  "hw.audioInput": "yes", "hw.battery": "yes",
  "hw.camera.back": "virtualscene", "hw.camera.front": "emulated",
  "hw.cpu.arch": "arm64", "hw.cpu.ncore": "4",
  "hw.dPad": "no", "hw.device.hash2": "MD5:7250072fa23402acb695d3eb0f172641",
  "hw.device.manufacturer": "Google", "hw.device.name": "pixel_4a",
  "hw.gps": "yes", "hw.gpu.enabled": "yes",
  "hw.gpu.mode": "auto", "hw.gyroscope": "yes",
  "hw.initialOrientation": "portrait", "hw.keyboard": "yes",
  "hw.lcd.density": "440", "hw.lcd.height": "2340",
  "hw.lcd.width": "1080", "hw.mainKeys": "no",
  "hw.ramSize": "2048", "hw.sdCard": "yes",
  "hw.sensors.light": "yes", "hw.sensors.magnetic_field": "yes",
  "hw.sensors.orientation": "yes", "hw.sensors.pressure": "yes",
  "hw.sensors.proximity": "yes", "hw.trackBall": "no",
  "image.sysdir.1": "system-images/android-36/google_apis_ps16k/arm64-v8a/", "runtime.network.latency": "none",
  "runtime.network.speed": "full", "sdcard.size": "512M",
  "showDeviceFrame": "yes", "skin.dynamic": "yes",
  "skin.name": "pixel_4a", "skin.path": "<fixture-sdk>/skins/pixel_4a",
  "tag.display": "16 KB Page Size", "tag.displaynames": "16 KB Page Size,Google APIs",
  "tag.id": "page_size_16kb", "tag.ids": "page_size_16kb,google_apis",
  "target": "android-36", "vm.heapSize": "256",
});

// Existing factory fixture; every SDK command below is a controlled port, never executed.
function fixture(t, options = {}) {
  const directory = fs.mkdtempSync(resolve(tmpdir(), 'owned-avd-creator-control-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const sdkRoot = resolve(directory, 'sdk'), javaHome = resolve(directory, 'jdk21');
  const sdk = { sdkRoot, emulatorPath: resolve(sdkRoot, 'emulator/emulator') };
  const image = resolve(sdkRoot, OWNED_ANDROID_SPEC.image), referenceConfig = resolve(directory, 'reference.ini');
  const events = [], commands = [], intents = [];
  const contract = {
    'abi.type': 'arm64-v8a', 'hw.cpu.arch': 'arm64', 'hw.device.name': 'pixel_4a',
    'hw.device.hash2': 'MD5:7250072fa23402acb695d3eb0f172641', 'tag.id': 'page_size_16kb', 'tag.ids': 'page_size_16kb,google_apis',
    'hw.lcd.width': '1080', 'hw.lcd.height': '2340', 'hw.lcd.density': '440', 'hw.ramSize': '2048',
    'hw.cpu.ncore': '4', 'hw.keyboard': 'yes', 'hw.gpu.enabled': 'yes', 'hw.gpu.mode': 'auto',
    'disk.dataPartition.size': '6442450944', 'hw.sdCard': 'yes', 'sdcard.size': '512M', 'skin.name': 'pixel_4a',
    'skin.path': resolve(sdkRoot, 'skins/pixel_4a'), 'PlayStore.enabled': 'false', 'hw.camera.back': 'virtualscene',
    'hw.camera.front': 'emulated', 'hw.initialOrientation': 'portrait', 'AvdId': 'Pixel_4a', 'avd.ini.displayname': 'Pixel 4a',
    'image.sysdir.1': OWNED_ANDROID_SPEC.image + '/', 'unchanged.readonly': 'preserved',
  };
  if (options.sdk04) {
    delete contract['unchanged.readonly']; Object.assign(contract, SDK04_REFERENCE, { 'skin.path': resolve(sdkRoot, 'skins/pixel_4a') });
  }
  function write(path, bytes) { fs.mkdirSync(resolve(path, '..'), { recursive: true }); fs.writeFileSync(path, bytes); }
  const encode = values => Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join('');
  write(referenceConfig, encode(contract));
  write(resolve(image, 'package.xml'), `<localPackage path="${OWNED_ANDROID_SPEC.package}"/>`);
  for (const path of [sdk.emulatorPath, resolve(sdkRoot, 'cmdline-tools/latest/bin/avdmanager'),
    resolve(sdkRoot, 'emulator/mksdcard'), resolve(image, 'data/empty_data_disk'), resolve(image, 'encryptionkey.img'),
    resolve(javaHome, 'bin/java')]) write(path, 'controlled immutable input, never executed');
  write(resolve(javaHome, 'release'), 'JAVA_VERSION="21.0.8"\n');
  const primary = new Error('controlled creator primary');
  let releaseCreator;
  const runCommand = async (file, args, commandOptions) => {
    events.push('creator-start'); commands.push({ file, args: [...args], options: commandOptions });
    assert.equal(intents.length, 1, 'Intent must exist before any SDK writer');
    const at = flag => args[args.indexOf(flag) + 1], content = at('-p'), name = at('-n');
    if (options.requireAbsentTarget) assert.equal(fs.existsSync(content), false,
      'Official creator needs an absent target; an existing directory selects duplicate-AVD mode');
    assert.equal(file, resolve(sdkRoot, 'cmdline-tools/latest/bin/avdmanager'));
    assert(!args.includes('-f') && !args.includes('--force'));
    assert(!Object.hasOwn(commandOptions.env, 'HOME'), 'OS HOME is not repurposed');
    const changedDefaults = Object.fromEntries(Object.keys(contract).filter(key => !['abi.type', 'hw.cpu.arch', 'hw.device.name', 'hw.device.hash2',
      'tag.id', 'tag.ids', 'AvdId', 'avd.ini.displayname', 'image.sysdir.1', 'unchanged.readonly'].includes(key)).map(key => [key, 'controlled-different-default']));
    const actual = options.sdk04 ? { ...SDK04_GENERATED }
      : { ...contract, ...changedDefaults, AvdId: name, 'avd.ini.displayname': name, 'sdcard.path': resolve(content, 'sdcard.img'), ...options.generated };
    options.amend?.(actual, { content, name });
    write(resolve(content, 'config.ini'), encode(actual));
    write(resolve(commandOptions.env.ANDROID_AVD_HOME, name + '.ini'), `path=${content}\n`);
    const fd = fs.openSync(resolve(content, 'sdcard.img'), 'w'); fs.ftruncateSync(fd, 512 * 1024 ** 2); fs.closeSync(fd);
    if (options.sdcardSymlink) { fs.unlinkSync(resolve(content, 'sdcard.img')); fs.symlinkSync(referenceConfig, resolve(content, 'sdcard.img')); }
    if (options.pending) await new Promise(resolveCreator => { releaseCreator = resolveCreator; });
    events.push('creator-close');
    if (options.fail) throw primary;
    return { stdout: 'created', stderr: '' };
  };
  const request = { runId: 'controlled-range', avdName: 'Pixel_4a', sdk, javaHome, referenceConfig,
    expectedFixtureHash: androidProfileHash(fs.readFileSync(referenceConfig)),
    expectedPackageHash: androidProfileHash(fs.readFileSync(resolve(image, 'package.xml'))), privateParent: directory,
    runCommand, onIntent: lease => { events.push('intent'); intents.push(lease); } };
  return { directory, request, contract, commands, intents, events, primary, write, release: () => releaseCreator(),
    start: () => createRunOwnedAndroidProfile(request) };
}

test('official creator binds exact supported arguments, private environment and 17-value normalization', async t => {
  const f = fixture(t, { requireAbsentTarget: true }), baseline = fs.readFileSync(f.request.referenceConfig), lease = await f.start(), profile = lease.verify();
  const { args, options } = f.commands[0];
  assert.deepEqual(args, ['create', 'avd', '-n', profile.physicalAvdName, '-k', OWNED_ANDROID_SPEC.package,
    '-d', 'pixel_4a', '-b', 'arm64-v8a', '-c', '512M', '-p', profile.content]);
  assert.equal(options.timeout, 120_000); assert.equal(options.env.JAVA_HOME, f.request.javaHome);
  for (const key of ['ANDROID_AVD_HOME', 'ANDROID_USER_HOME', 'ANDROID_EMULATOR_HOME', 'ANDROID_SDK_HOME', 'XDG_CACHE_HOME']) {
    assert(options.env[key].startsWith(profile.root + '/'));
  }
  const after = androidProfileIni(fs.readFileSync(profile.configPath, 'utf8'));
  for (const [key, value] of Object.entries(f.contract)) {
    if (['AvdId', 'avd.ini.displayname'].includes(key)) continue;
    assert.equal(after[key], value, key);
  }
  assert.deepEqual(fs.readFileSync(f.request.referenceConfig), baseline);
  assert(!fs.existsSync(resolve(f.request.sdk.sdkRoot, OWNED_ANDROID_SPEC.image, 'userdata.img')));
  assert(profile.launchArgs.includes('-no-snapstorage')); assert(profile.launchArgs.includes('-no-snapshot'));
  assert.equal(typeof JSON.parse(JSON.stringify(lease)).remove, 'undefined');
  lease.remove(); assert(!fs.existsSync(profile.root)); lease.remove();
});

for (const key of ['abi.type', 'hw.cpu.arch', 'hw.device.name', 'hw.device.hash2', 'tag.id', 'tag.ids']) {
  test(`official creator refuses identity mismatch ${key} without relabeling it`, async t => {
    const f = fixture(t, { generated: { [key]: 'foreign' } });
    await assert.rejects(f.start(), /identity mismatch/);
    assert.equal(f.commands.length, 1); assert(!fs.existsSync(f.intents[0].intent.root));
  });
}
for (const generated of [{ 'unchanged.readonly': 'different' }, { 'new.unclassified': 'value' },
  { 'sdcard.path': '/foreign/sdcard.img' }, { 'image.sysdir.1': 'system-images/another' }]) {
  test(`official creator fails closed on unclassified or foreign default ${Object.keys(generated)[0]}`, async t => {
    const f = fixture(t, { generated }); await assert.rejects(f.start());
    assert(!fs.existsSync(f.intents[0].intent.root));
  });
}
test('partial create retains root until actual command-close promise and preserves primary', async t => {
  const f = fixture(t, { pending: true, fail: true });
  const pending = f.start(); await new Promise(resolveTurn => setImmediate(resolveTurn));
  const lease = f.intents[0]; assert(fs.existsSync(lease.intent.root));
  assert.throws(() => lease.remove(), /not closed/);
  f.release(); await assert.rejects(pending, error => error === f.primary);
  assert.deepEqual(f.events, ['intent', 'creator-start', 'creator-close']);
  assert(!fs.existsSync(lease.intent.root));
});
test('creator never touches a reference fixture on pinned input drift', async t => {
  const f = fixture(t); f.write(f.request.referenceConfig, 'changed');
  await assert.rejects(f.start(), /inputs changed/); assert.equal(f.commands.length, 0); assert.equal(f.intents.length, 0);
});
test('owned root replacement is refused rather than recursively deleted', async t => {
  const f = fixture(t), lease = await f.start(), root = lease.intent.root;
  fs.renameSync(root, root + '-original'); fs.mkdirSync(root);
  await assert.rejects(async () => lease.remove(), /identity changed/); assert(fs.existsSync(root + '-original'));
});
test('SDK-created SD metadata cannot substitute a symlink outside the own root', async t => {
  const f = fixture(t, { sdcardSymlink: true }), original = fs.readFileSync(f.request.referenceConfig);
  await assert.rejects(f.start(), /symlink refused/);
  assert.deepEqual(fs.readFileSync(f.request.referenceConfig), original); assert(!fs.existsSync(f.intents[0].intent.root));
});

// These cases call the same actual factory with an independently captured SDK shape, not a production allowlist.
function sdk04Fixture(t, amend = () => {}) { return fixture(t, { sdk04: true, requireAbsentTarget: true, amend }); }
function immutableInputs(f) {
  const sdk = f.request.sdk.sdkRoot, image = resolve(sdk, OWNED_ANDROID_SPEC.image), jdk = f.request.javaHome;
  return [f.request.referenceConfig, f.request.sdk.emulatorPath, resolve(sdk, 'cmdline-tools/latest/bin/avdmanager'),
    resolve(sdk, 'emulator/mksdcard'), resolve(image, 'package.xml'), resolve(image, 'data/empty_data_disk'),
    resolve(image, 'encryptionkey.img'), resolve(jdk, 'release'), resolve(jdk, 'bin/java')].map(path => [path, fs.readFileSync(path)]);
}
async function rejectSdk04(t, amend, key) {
  const f = sdk04Fixture(t, amend), inputs = immutableInputs(f);
  await assert.rejects(f.start(), error => error instanceof Error && error.message.includes(key));
  assert.equal(f.commands.length, 1); assert.deepEqual(f.events, ['intent', 'creator-start', 'creator-close']);
  assert(!fs.existsSync(f.intents[0].intent.root)); assert.deepEqual(immutableInputs(f), inputs);
}
function assertCanonicalSdk04(f, profile) {
  const values = androidProfileIni(fs.readFileSync(profile.configPath, 'utf8'));
  assert.equal(Object.keys(values).length, 57, 'All54 reference fields plus2 hardware name aliases and1 owned data path');
  for (const [key, expected] of Object.entries(f.contract)) {
    assert.equal(values[key], ['AvdId', 'avd.ini.displayname'].includes(key) ? profile.physicalAvdName : expected, key);
  }
  assert.deepEqual(Object.keys(values).filter(key => !Object.hasOwn(f.contract, key)).sort(), ['avd.id', 'avd.name', 'disk.dataPartition.path']);
  for (const key of ['AvdId', 'avd.ini.displayname', 'avd.id', 'avd.name']) assert.equal(values[key], profile.physicalAvdName, key);
  assert.equal(values['disk.dataPartition.path'], resolve(profile.content, 'userdata-qemu.img'));
  assert.equal(profile.launchArgs[profile.launchArgs.indexOf('-data') + 1], values['disk.dataPartition.path']);
  assert(profile.launchArgs.includes('-no-snapshot')); assert(profile.launchArgs.includes('-no-snapstorage'));
  assert.equal(fs.statSync(resolve(profile.content, 'sdcard.img')).size, 512 * 1024 ** 2);
}

test('SDK04 complete official shape preserves the canonical reference and exact owned identity', async t => {
  assert.equal(Object.keys(SDK04_GENERATED).length, 138); assert.equal(Object.keys(SDK04_REFERENCE).length, 54);
  const f = sdk04Fixture(t), inputs = immutableInputs(f), lease = await f.start(), profile = lease.verify();
  assertCanonicalSdk04(f, profile); assert.deepEqual(immutableInputs(f), inputs);
  assert(!fs.existsSync(resolve(f.request.sdk.sdkRoot, OWNED_ANDROID_SPEC.image, 'userdata.img')));
  lease.remove(); assert(!fs.existsSync(profile.root)); assert.deepEqual(immutableInputs(f), inputs);
});
test('SDK04 foreign partial or conflicting name pairs cannot be relabeled as this owned profile', async t => {
  for (const amend of [a => Object.assign(a, { AvdId: 'foreign', 'avd.ini.displayname': 'foreign' }),
    a => { a.AvdId = 'foreign'; }, a => { a['avd.id'] = 'foreign'; }, a => { delete a['avd.name']; }]) {
    await rejectSdk04(t, amend, 'name');
  }
});
test('SDK04 only the observed arc and heap representations may map to the reference', async t => {
  for (const [key, value] of [['hw.arc', 'yes'], ['vm.heapSize', '512M']]) await rejectSdk04(t, a => { a[key] = value; }, key);
});
test('SDK04 conflicting snapshot skin or target metadata is refused before canonicalization', async t => {
  for (const [key, value] of [['fastboot.chosenSnapshotFile', 'foreign-snapshot'], ['skin.dynamic', 'no'], ['target', 'android-35']]) {
    await rejectSdk04(t, a => { a[key] = value; }, key);
  }
});
test('SDK04 a changed known default cannot hide inside the default-only drop set', async t => {
  for (const key of ['hw.audioOutput', 'hw.keyboard.lid']) await rejectSdk04(t, a => { a[key] = 'no'; }, key);
});
test('SDK04 unknown fields and unlisted writable paths remain fail closed', async t => {
  for (const [key, value] of [['unlisted.setting', 'value'], ['hw.logcatOutput.path', '/foreign/log.txt']]) {
    await rejectSdk04(t, a => { a[key] = value; }, key);
  }
});
test('SDK04 absent temp or exact owned data inputs converge to the actual data argv', async t => {
  for (const mode of ['absent', 'temp', 'owned']) {
    const f = sdk04Fixture(t, (a, { content }) => {
      if (mode === 'absent') delete a['disk.dataPartition.path'];
      else a['disk.dataPartition.path'] = mode === 'temp' ? '<temp>' : resolve(content, 'userdata-qemu.img');
    });
    const inputs = immutableInputs(f), lease = await f.start(), profile = lease.verify();
    assertCanonicalSdk04(f, profile); assert.deepEqual(immutableInputs(f), inputs); lease.remove(); assert(!fs.existsSync(profile.root));
  }
});
test('SDK04 foreign traversal or different owned data destinations cannot be silently overridden', async t => {
  for (const mode of ['foreign', 'traversal', 'different-owned']) {
    await rejectSdk04(t, (a, { content }) => {
      a['disk.dataPartition.path'] = mode === 'foreign' ? '/foreign/data.img' : mode === 'traversal' ? '../escape.img' : resolve(content, 'other.img');
    }, 'disk.dataPartition.path');
  }
});
