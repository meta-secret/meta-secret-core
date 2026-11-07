import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import * as supervisor from './platform-supervisor.mjs';
import { resetE2eNativeAppState, resetE2eScenarioState } from './e2e-state-reset.mjs';
import { preflightBatchEnvironment } from './batch-environment.mjs';

const runId = 'controlled-owned-ios';
const profile = { runId, simulatorName: 'iPhone 17', name: `MetaSecret-e2e-${runId}-fixture`,
  udid: '11111111-1111-4111-8111-111111111111', runtimeIdentifier: 'com.apple.CoreSimulator.SimRuntime.iOS-26-5',
  deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17', createdByRun: true, absentBeforeCreate: true };
const device = { name: profile.name, udid: profile.udid, state: 'Booted', isAvailable: true, deviceTypeIdentifier: profile.deviceTypeIdentifier };
const inventory = { devices: { [profile.runtimeIdentifier]: [device,
  { ...device, name: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30' }] } };

function resetFixture(t, changed = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'owned-ios-reset-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const readinessPath = join(directory, 'readiness.json');
  const platform = { platform: 'iOS', simulatorName: 'iPhone 17', udid: profile.udid, ownedIosProfile: profile, ...changed };
  writeFileSync(readinessPath, JSON.stringify({ runId, platforms: [{ ...platform, ownedIosProfile: profile, udid: profile.udid }] }));
  const calls = [];
  const runCommand = async (_file, args) => {
    calls.push(args);
    if (args[1] === 'list') return { stdout: JSON.stringify(inventory) };
    if (args[1] === 'get_app_container') throw new Error('Application is not installed');
    if (args[1] === 'keychain') return { stdout: '' };
    if (args[1] === 'spawn') return { stdout: '96981\n' };
    throw new Error('Unexpected controlled command: ' + args[1]);
  };
  return { calls, platform, runCommand, readinessPath,
    reset: () => resetE2eNativeAppState({ scenario: { ios: { bundleId: 'org.metasecret.vault' } },
      platforms: [platform], runId, readinessPath, runCommand }) };
}

test('owned iOS selection uses exact UUID/runtime/type instead of an existing same logical name', async () => {
  const calls = [];
  const result = await supervisor.prepareIos('iPhone 17', () => assert.fail('already Booted'), {
    ownedIosProfile: profile, runId,
    runCommand: async (_file, args) => { calls.push(args); return { stdout: args[1] === 'list' ? JSON.stringify(inventory) : args[1] === 'spawn' ? '96981\n' : '' }; },
    openDeviceUi: async ({ simulatorUdid }) => { assert.equal(simulatorUdid, profile.udid); return {}; },
  });
  assert.equal(result.udid, profile.udid);
  assert.deepEqual(result.ownedIosProfile, profile);
  assert(calls.filter(args => args[1] !== 'list').every(args => args.includes(profile.udid)));
});

test('foreign owned iOS run claim rejects before boot, heartbeat or device UI', async () => {
  const calls = [];
  await assert.rejects(supervisor.prepareIos('iPhone 17', () => assert.fail('no boot'), {
    ownedIosProfile: { ...profile, runId: 'another-run' }, runId,
    runCommand: async (_file, args) => { calls.push(args); return { stdout: args[1] === 'list' ? JSON.stringify(inventory) : '96981\n' }; },
    openDeviceUi: async () => assert.fail('foreign profile must not open'),
  }), /owned iOS profile/i);
  assert(calls.every(args => args[1] === 'list'));
});

test('legacy owned iOS app-absent reset is refused before any app or Keychain command', async t => {
  const f = resetFixture(t); await assert.rejects(f.reset(), /Legacy owned profile/);
  assert.deepEqual(f.calls, []);
});

test('foreign owned iOS reset claim fails before even inspecting an app container', async t => {
  const f = resetFixture(t, { udid: '22222222-2222-4222-8222-222222222222' });
  await assert.rejects(f.reset(), /Legacy owned profile/i);
  assert(f.calls.every(args => args[1] === 'list'));
});

test('default unowned iOS reset requires isolated identity before any command and never clears a Keychain', async t => {
  const f = resetFixture(t); delete f.platform.ownedIosProfile; f.platform.udid = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';
  await assert.rejects(f.reset(), /Isolated iOS identity context is required before reset/);
  assert.deepEqual(f.calls, []);
  assert(!f.calls.some(args => args[1] === 'keychain'));
});

function creationFixture(mode) {
  const calls = []; let created; let postFailure = mode === 'post-inspection';
  const primary = new Error('controlled post-create inventory failure');
  const foreign = structuredClone(inventory.devices[profile.runtimeIdentifier][1]);
  const devices = [foreign];
  const runCommand = async (_file, args) => {
    calls.push(args);
    if (args[1] === 'list' && args[2] === 'runtimes') return { stdout: JSON.stringify({ runtimes: [{ identifier: profile.runtimeIdentifier, isAvailable: true }] }) };
    if (args[1] === 'list' && args[2] === 'devicetypes') return { stdout: JSON.stringify({ devicetypes: [{ identifier: profile.deviceTypeIdentifier }] }) };
    if (args[1] === 'list') {
      if (created && postFailure) { postFailure = false; throw primary; }
      return { stdout: JSON.stringify({ devices: { [profile.runtimeIdentifier]: devices } }) };
    }
    if (args[1] === 'create') {
      if (mode === 'existing-uuid') return { stdout: foreign.udid };
      created = { ...device, state: 'Shutdown', name: args[2] }; devices.push(created);
      return { stdout: created.udid };
    }
    assert.equal(args[2], created?.udid, 'Only exact created UUID may be mutated');
    if (args[1] === 'shutdown') { created.state = 'Shutdown'; return { stdout: '' }; }
    if (args[1] === 'delete') { devices.splice(devices.indexOf(created), 1); return { stdout: '' }; }
    throw new Error('Unexpected controlled creation command');
  };
  return { calls, devices, primary, runCommand, foreign, created: () => created };
}

test('created iOS UUID is published before post-create failure and only that exact profile is cleaned', async () => {
  const f = creationFixture('post-inspection'); let published;
  await assert.rejects(supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand, onCreated: value => { published = value; } }), error => error === f.primary);
  assert.equal(published.udid, profile.udid);
  assert.deepEqual(f.calls.filter(args => args[1] === 'delete'), [['simctl', 'delete', profile.udid]]);
  assert.deepEqual(f.devices, [f.foreign]);
});

test('owned profile cleanup shuts down and proves deletion while preserving existing profiles', async () => {
  const f = creationFixture(); const owned = await supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand });
  f.created().state = 'Booted'; await supervisor.deleteRunOwnedIosProfile(owned, { runId, runCommand: f.runCommand });
  assert.deepEqual(f.calls.filter(args => ['shutdown', 'delete'].includes(args[1])), [
    ['simctl', 'shutdown', owned.udid], ['simctl', 'delete', owned.udid],
  ]);
  assert.deepEqual(f.devices, [f.foreign]);
});

test('creation refuses a pre-existing returned UUID without ever deleting it', async () => {
  const f = creationFixture('existing-uuid');
  await assert.rejects(supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand }), /pre-existing/);
  assert(!f.calls.some(args => ['shutdown', 'delete'].includes(args[1]))); assert.deepEqual(f.devices, [f.foreign]);
});

test('foreign cleanup claim is rejected before any SDK command', async () => {
  await assert.rejects(supervisor.deleteRunOwnedIosProfile({ ...profile, runId: 'foreign' }, {
    runId, runCommand: async () => assert.fail('No command for a foreign profile'),
  }), /owned iOS profile/);
});

test('exact iOS selector rejects current inventory runtime/type/name or UUID ambiguity', () => {
  for (const field of ['runtime', 'deviceTypeIdentifier', 'name', 'duplicate']) {
    const view = structuredClone(inventory); const selected = view.devices[profile.runtimeIdentifier][0];
    if (field === 'runtime') { view.devices.foreign = view.devices[profile.runtimeIdentifier]; delete view.devices[profile.runtimeIdentifier]; }
    else if (field === 'duplicate') view.devices[profile.runtimeIdentifier].push({ ...selected });
    else selected[field] = 'foreign';
    assert.throws(() => supervisor.selectIosSimulator({ simulatorName: 'iPhone 17', inventory: view, ownedIosProfile: profile, runId }), /owned iOS profile|ambiguous/);
  }
});

test('readiness UUID must agree with its own profile metadata before a child can select it', t => {
  const f = resetFixture(t); const report = { runId, platforms: [{ ...f.platform, udid: '22222222-2222-4222-8222-222222222222' }] };
  writeFileSync(f.readinessPath, JSON.stringify(report));
  assert.throws(() => supervisor.resolveIosScenarioSimulator({ simulatorName: 'iPhone 17', inventory, readinessPath: f.readinessPath, runId }), /Legacy owned profile/);
});

test('forbidden Keychain reset never invokes a transport or reports readiness', async t => {
  const f = resetFixture(t); await assert.rejects(f.reset(), /Legacy owned profile/);
  assert.deepEqual(f.calls, []);
});

test('actual Test4 per-block preparation refuses legacy ownership before bootstatus or reset', async t => {
  const f = resetFixture(t); const events = [];
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('async function prepareIosSimulator() {');
  const end = source.indexOf('\nasync function prepareIosTestBundle(', start);
  assert(start >= 0 && end > start, 'Actual named preparation body must exist');
  const prepare = runInNewContext(source.slice(start, end) + '\nprepareIosSimulator', {
    assertExistingDeviceRun: supervisor.assertExistingDeviceRun,
    findIosSimulator: async () => ({ ...device, ownedIosProfile: profile }),
    runAndWait: async () => { events.push('bootstatus'); }, console: { log() {} }, writeDiagnostic() {},
    scenario: { ios: { bundleId: 'org.metasecret.vault' } },
    resetE2eNativeAppState: async options => { events.push('reset'); return resetE2eNativeAppState({ ...options, runId, readinessPath: f.readinessPath, runCommand: f.runCommand }); },
    openIosDeviceUi: async udid => { assert.equal(udid, profile.udid); events.push('visible'); },
  });
  await assert.rejects(prepare(), /Legacy owned profile/);
  assert.deepEqual(events, []); assert.deepEqual(f.calls, []);
  const mainPreparation = source.lastIndexOf('const simulatorUdid = await prepareIosSimulator();');
  assert(mainPreparation > end && source.indexOf('await installBuiltIosTestApps({', mainPreparation) > mainPreparation);
});

test('owned-profile preflight is refused before any SDK or service inventory', async () => {
  const scenario = JSON.parse(readFileSync(new URL('../Test1/scenario.json', import.meta.url), 'utf8'));
  await assert.rejects(preflightBatchEnvironment([scenario], { ownedIosProfile: true,
    runCommand: async () => assert.fail('No command for a forbidden owned-profile request') }), /forbidden/);
});

test('creation cleanup failure retains original cause and never substitutes another profile', async () => {
  const f = creationFixture(); const primary = new Error('controlled owner publication failure');
  await assert.rejects(supervisor.createRunOwnedIosProfile({ runId, onCreated: () => { throw primary; },
    runCommand: async (...args) => { if (args[1][1] === 'delete') throw new Error('controlled exact deletion failure'); return f.runCommand(...args); },
  }), error => error instanceof AggregateError && error.cause === primary && error.errors[0] === primary);
  assert(f.devices.includes(f.foreign));
});

test('cleanup absence proof includes unavailable profiles and cannot confuse a filtered list with deletion', async () => {
  const f = creationFixture(); const owned = await supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand });
  f.created().isAvailable = false;
  await assert.rejects(supervisor.deleteRunOwnedIosProfile(owned, { runId,
    runCommand: async (file, args) => {
      if (args[1] === 'delete') return { stdout: '' };
      if (args.includes('available')) return { stdout: JSON.stringify({ devices: { [profile.runtimeIdentifier]: [f.foreign] } }) };
      return f.runCommand(file, args);
    },
  }), /still exists/);
});

test('foreign profile stops scenario reset before even server-container mutation', async t => {
  const f = resetFixture(t, { ownedIosProfile: { ...profile, runId: 'foreign' } });
  await assert.rejects(resetE2eScenarioState({ scenario: { server: { container: 'meta-secret-e2e-server-owned-test', url: 'http://127.0.0.1:3000' } },
    platforms: [f.platform], runId, readinessPath: f.readinessPath, runCommand: async () => assert.fail('No server/device command for foreign claim'),
  }), /Legacy owned profile/);
});

for (const outcome of ['throw', 'invalid-stdout']) test(`uncertain create ${outcome} recovers only new nonce UUID before cleanup`, async () => {
  const f = creationFixture(); let published; let intent;
  await assert.rejects(supervisor.createRunOwnedIosProfile({ runId, onCreateIntent: value => { intent = value; }, onCreated: value => { published = value; },
    runCommand: async (...args) => {
      if (args[1][1] === 'create') { assert.equal(intent.name, args[1][2]); assert(intent.baselineUdids.includes(f.foreign.udid)); }
      const result = await f.runCommand(...args);
      if (args[1][1] === 'create') { if (outcome === 'throw') throw new Error('controlled uncertain creation'); return { stdout: '' }; }
      return result;
    },
  }));
  assert.deepEqual(f.devices, [f.foreign], 'Actual create succeeded despite an unusable command outcome; its own profile must not leak');
  assert.equal(published.udid, profile.udid);
});

test('shutdown error with finite transitional state still settles same owned UUID before deletion', async () => {
  const f = creationFixture(); const owned = await supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand });
  f.created().state = 'Booted'; let transitioning = false;
  await assert.doesNotReject(supervisor.deleteRunOwnedIosProfile(owned, { runId,
    runCommand: async (...args) => {
      if (args[1][1] === 'shutdown') { transitioning = true; f.created().state = 'Shutting Down'; throw new Error('controlled shutdown command error'); }
      if (transitioning && args[1][1] === 'list') f.created().state = 'Shutdown';
      return f.runCommand(...args);
    },
  }));
  assert.deepEqual(f.devices, [f.foreign]);
});

test('owned shutdown deadline retains the command cause and never deletes an unsettled profile', async () => {
  const f = creationFixture(); const owned = await supervisor.createRunOwnedIosProfile({ runId, runCommand: f.runCommand });
  f.created().state = 'Booted'; const primary = new Error('controlled cannot shutdown'); let clock = 0;
  await assert.rejects(supervisor.deleteRunOwnedIosProfile(owned, { runId, now: () => clock, wait: async ms => { clock += ms; },
    runCommand: async (...args) => { if (args[1][1] === 'shutdown') throw primary; return f.runCommand(...args); },
  }), error => /shut down/.test(error.message) && error.cause === primary);
  assert(!f.calls.some(args => args[1] === 'delete'));
});

for (const mismatch of ['type', 'ambiguous']) test(`uncertain create ${mismatch} never adopts a profile merely by nonce name`, async () => {
  const f = creationFixture(); const primary = new Error('controlled uncertain creation');
  await assert.rejects(supervisor.createRunOwnedIosProfile({ runId,
    runCommand: async (...args) => {
      const result = await f.runCommand(...args);
      if (args[1][1] === 'create') {
        if (mismatch === 'type') f.created().deviceTypeIdentifier = 'foreign-type';
        else f.devices.push({ ...f.created(), udid: '33333333-3333-4333-8333-333333333333' });
        throw primary;
      }
      return result;
    },
  }), error => error instanceof AggregateError && error.cause === primary);
  assert(!f.calls.some(args => ['shutdown', 'delete'].includes(args[1])));
});
