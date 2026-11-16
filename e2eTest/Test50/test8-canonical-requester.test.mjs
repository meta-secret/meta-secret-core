import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { createStepEnvironment } from '../shared/ios-step-secret.mjs';

const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
const scenario = JSON.parse(readFileSync(new URL('../Test8/scenario.json', import.meta.url), 'utf8'));
const start = source.indexOf('async function runBothReceiversOfflineRecovery(');
const end = source.indexOf('\nasync function runSenderOfflineSetup(', start);
assert(start >= 0 && end > start, 'actual whole Test8 body must be present');
const body = source.slice(start, end).trim();
const launcher = source.indexOf('function startIosJoinTest(');
const serializeStart = source.indexOf('    stepConfiguration = {', launcher);
const serializeEnd = source.indexOf('    writeIosStepConfiguration(stepConfiguration);', serializeStart);
assert(serializeStart > launcher && serializeEnd > serializeStart, 'actual iOS configuration body must be present');
const serialize = runInNewContext(`(expectedDeviceNames, environment) => {
  let stepConfiguration;
  ${source.slice(serializeStart, serializeEnd)}
  return stepConfiguration;
}`, { declaredDevicePasscodes: {}, persistedCoreServerUrl: '', scenario,
  deletionAuthProfile: undefined, selectedSecret: { name: 'fixture-secret', value: 'fixture-value' } });

const categories = { Web: 'web', Android: 'android', iPhone: 'ios' };
const members = {
  Web: { deviceId: 'fixture-id-one', deviceName: 'canonical-one' },
  Android: { deviceId: 'fixture-id-two', deviceName: 'canonical-two' },
  iPhone: { deviceId: 'fixture-id-three', deviceName: 'canonical-three' },
};
const expectedNames = { web: 'canonical-one', android: 'canonical-two', ios: 'canonical-three' };
const plain = value => JSON.parse(JSON.stringify(value));

async function exercise({ overrides = {}, readerError } = {}) {
  const events = [], launches = [], markers = [], discovery = [], phases = [];
  const actor = (platform, options) => {
    const environment = platform === 'ios' ? options.environment : createStepEnvironment(options);
    const row = { platform, options, role: environment.E2E_ROLE,
      cycle: Number(environment.E2E_CYCLE), step: Number(environment.E2E_STEP),
      sender: environment.E2E_SENDER, approval: environment.E2E_APPROVAL_PLATFORM,
      alreadySent: environment.E2E_RECEIVER_REQUEST_ALREADY_SENT };
    if (platform === 'ios') row.persisted = serialize(options.expectedDeviceNames, environment);
    launches.push(row); events.push({ kind: 'launch', platform, cycle: row.cycle, role: row.role });
    return { result: Promise.resolve(), waitForMarker: async (marker, timeout) => {
      markers.push({ platform, marker, timeout, cycle: row.cycle });
    } };
  };
  const ui = name => async (...args) => { events.push({ kind: name }); return args; };
  const ports = {
    scenario, defaultSecret: { name: 'fixture-secret' }, console: { log() {} },
    inspectWebLiveboardMember: async (_page, category, options) => {
      discovery.push({ category, options: plain(options) }); events.push({ kind: 'inspect', category });
      if (readerError) throw readerError;
      return overrides[category] ?? { ...members[category] };
    },
    setCurrentCycleForDiagnostics: cycle => phases.push(cycle),
    stopAndroidApplication: ui('stopAndroid'), stopIosApplication: ui('stopIos'),
    stopWebApplication: ui('stopWeb'), reopenWebAfterOffline: ui('reopenWeb'),
    startWebRecovery: ui('webRequest'), revealAndCloseWebSecret: ui('webRevealClose'),
    waitForWebIncomingRecoveryCount: ui('webIncoming'), approveIncomingRecoveryOnWeb: ui('webApprove'),
    waitForWebIncomingRecoveryGone: ui('webGone'),
    stepEnvironment: createStepEnvironment,
    startIosJoinTest: (_udid, options) => actor('ios', options),
    startAndroidStepTest: async (_serial, options) => actor('android', options),
    approvalCoordinator: { allow: (platform, cycle) => events.push({ kind: 'permit', platform, cycle }),
      publishFifoIdentities: () => assert.fail('Test8 must not publish generic FIFO identities') },
    concurrentFifoIdentities: () => assert.fail('Test8 must not enter generic FIFO protocol'),
  };
  const discoverySource = actualFunction('discoverCanonicalRecoveryDeviceNames', true);
  if (discoverySource) ports.discoverCanonicalRecoveryDeviceNames = runInNewContext('(' + discoverySource + ')', ports);
  const operation = runInNewContext(`(${body})`, ports);
  let error;
  try { await operation({}, 'closed-ios', 'closed-android'); } catch (caught) { error = caught; }
  return { events, launches, markers, discovery, phases, error };
}

function originalTranscript(result) {
  assert.equal(result.error, undefined, 'actual whole Test8 must finish its positive transcript');
  assert.equal(scenario.recovery.expectedCycles ?? (scenario.recovery.expectedBlockCount ?? 3) * 6, 18);
  assert.equal(scenario.recovery.offlineBlocks.length, 3);
  assert(scenario.recovery.offlineBlocks.every(block => block.count === 3));
  assert.deepEqual(result.phases, Array.from({ length: 18 }, (_, index) => index + 1));
  assert.equal(result.launches.length, 36);
  const counts = {};
  for (const row of result.launches) counts[`${row.platform}-${row.role}`] = (counts[`${row.platform}-${row.role}`] ?? 0) + 1;
  assert.deepEqual(counts, { 'android-receiver': 12, 'ios-receiver': 12, 'ios-sender': 6, 'android-sender': 6 });
  assert.equal(result.events.filter(row => row.kind === 'webRequest').length, 6);
  assert.equal(result.events.filter(row => row.kind === 'webRevealClose').length, 6);
  assert.equal(result.events.filter(row => row.kind === 'webApprove').length, 6);
  for (const cycle of result.phases) {
    const block = scenario.recovery.offlineBlocks[Math.floor((cycle - 1) / 6)];
    const step = (cycle - 1) % 2 + 1;
    const approver = block.approvalOrder[step - 1];
    const nonApprover = block.receivers.find(platform => platform !== approver);
    const actors = result.launches.filter(row => row.cycle === cycle);
    for (const row of actors) {
      assert.equal(row.step, step); assert.equal(row.sender, block.sender); assert.equal(row.approval, approver);
      if (row.role === 'receiver') assert.equal(row.alreadySent, '1');
      const expected = row.role === 'sender' ? `E2E: ${row.platform.toUpperCase()}_RECOVERY_REQUEST_SENT_${cycle}_${step}`
        : `E2E: ${row.platform.toUpperCase()}_INCOMING_VISIBLE_${cycle}_${step}`;
      assert(result.markers.some(marker => marker.platform === row.platform && marker.marker === expected));
    }
    const permits = result.events.filter(row => row.kind === 'permit' && row.cycle === cycle).map(row => row.platform);
    assert.deepEqual(permits, [
      ...(block.sender === 'web' ? [] : [`${block.sender}-sender-${step}`]),
      ...(approver === 'web' ? [] : [`${approver}-approve-${step}`]),
      ...(nonApprover === 'web' ? [] : [`${nonApprover}-dismiss-${step}`]),
      ...(block.sender === 'web' ? [] : [`${block.sender}-show-${step}`]),
    ]);
  }
  assert(result.markers.every(row => row.timeout === 180_000));
}

test('actual Test8 completes all original 18 requests then requires canonical requester map on four launch sites', async () => {
  const result = await exercise();
  originalTranscript(result);
  for (const row of result.launches) {
    assert.deepEqual(plain(row.options.expectedDeviceNames ?? {}), expectedNames,
      `CANONICAL_REQUESTER_MAP_REQUIRED ${row.platform}-${row.role}`);
  }
});

test('actual Test8 discovers exactly three current canonical identities before business and freezes one shared map', async () => {
  const result = await exercise(); originalTranscript(result);
  assert.deepEqual(result.discovery, [
    { category: 'Web', options: { current: true, controls: false } },
    { category: 'Android', options: { current: false, controls: false } },
    { category: 'iPhone', options: { current: false, controls: false } },
  ]);
  assert(result.events.slice(0, 3).every(row => row.kind === 'inspect'));
  const first = result.launches[0].options.expectedDeviceNames;
  assert(first && Object.isFrozen(first));
  assert(result.launches.every(row => row.options.expectedDeviceNames === first));
  assert(Object.values(expectedNames).every(name => !/web|android|iphone|ios/i.test(name)));
  for (const row of result.launches.filter(row => row.platform === 'ios')) {
    assert.deepEqual([row.persisted.expectedWebDeviceName, row.persisted.expectedAndroidDeviceName,
      row.persisted.expectedIosDeviceName], Object.values(expectedNames));
    assert.equal(row.persisted.sender, row.sender);
    assert.equal(row.persisted.cycle, String(row.cycle));
    assert.equal(row.persisted.step, String(row.step));
  }
});

test('actual Test8 refuses invalid canonical identity before any launch, stop, recovery or permit', async t => {
  const cases = [
    ['missing ID', { Android: { ...members.Android, deviceId: '' } }],
    ['missing name', { Web: { ...members.Web, deviceName: '' } }],
    ['duplicate IDs', { iPhone: { ...members.iPhone, deviceId: members.Android.deviceId } }],
  ];
  for (const [label, overrides] of cases) await t.test(label, async () => {
    const result = await exercise({ overrides });
    assert(result.error, 'invalid canonical metadata must refuse');
    assert.equal(result.launches.length, 0);
    assert(result.events.every(row => row.kind === 'inspect'), 'refusal must precede all business operations');
  });
});

test('actual Test8 retains the exact discovery error and performs no business on missing or ambiguous member', async () => {
  for (const reason of ['missing member', 'ambiguous member', 'wrong current member']) {
    const primary = new Error(reason);
    const result = await exercise({ readerError: primary });
    assert.equal(result.error, primary);
    assert.equal(result.launches.length, 0);
    assert(result.events.every(row => row.kind === 'inspect'));
  }
});


// Evaluate actual metadata expressions with closed caller locals, not copied launch options.
function actualFunction(name, optional = false) {
  const start = source.indexOf('async function ' + name + '(');
  if (optional && start < 0) return null;
  const end = source.indexOf('\n}', start);
  assert(start >= 0 && end > start, 'actual function required: ' + name);
  return source.slice(start, end + 2);
}

function nativeOptions(name) {
  const text = actualFunction(name), rows = [];
  for (const match of text.matchAll(/(startIosJoinTest|startAndroidStepTest)\([^,]+,\s*\{|(startIosAction)\(\s*\{/g)) {
    const callee = match[1] ?? match[2];
    const start = match.index + match[0].lastIndexOf('{');
    let end = start + 1, depth = 1;
    while (depth && end < text.length) {
      if (text[end] === '{') depth += 1;
      if (text[end] === '}') depth -= 1;
      end += 1;
    }
    assert.equal(depth, 0, 'actual option literal must be complete');
    rows.push({ callee, platform: callee !== 'startAndroidStepTest' ? 'ios' : 'android', expression: text.slice(start, end) });
  }
  return rows;
}

async function optionLocals(number) {
  const currentScenario = JSON.parse(readFileSync(new URL('../Test' + number + '/scenario' +
    ([9, 10, 11, 12, 13, 14].includes(number) ? '-block1' : '') + '.json', import.meta.url), 'utf8'));
  const reader = async (_page, category, options) => {
    assert.deepEqual(plain(options), { current: category === 'Web', controls: false });
    return { ...members[category] };
  };
  const helper = actualFunction('discoverCanonicalRecoveryDeviceNames', true);
  // Before the helper exists this is closed caller metadata, never an implementation substitute.
  const names = helper ? await runInNewContext('(' + helper + ')', { inspectWebLiveboardMember: reader })({}) : expectedNames;
  const role = number === 16 ? 'offline-receiver' : 'receiver';
  const environment = createStepEnvironment({ role, cycle: 1, step: 1, sender: 'web', approvalPlatform: 'ios' });
  return { scenario: currentScenario, testNumber: number, cycle: { number: 1 }, first: { number: 1 },
    step: 1, sender: 'web', approver: 'ios', target: 'ios', receiver: 'ios', role,
    approval: { platform: 'ios', action: 'approve' }, expectedOutcome: 'approved',
    secretName: 'fixture-secret', block: 'fixture-block', cycleNumbers: '1', environment,
    expectedDeviceNames: names, stepEnvironment: createStepEnvironment };
}

const stableModes = [
  ['runSenderOfflineRecovery', 9, 6], ['runReceiverOfflineAfterAlertRecovery', 10, 2],
  ['runApproveDeclineRaceRecovery', 11, 2], ['runRepeatApproveRecovery', 13, 2],
  ['runDuplicateRecovery', 14, 2], ['runNetworkLossRecovery', 16, 2],
  ['runServerRestartRecovery', 17, 2], ['runMultipleWebTabsRecovery', 19, 2],
];

test('actual current Test9 receiver options preserve original role and serialization then require canonical map', async () => {
  const rows = nativeOptions('runSenderOfflineRecovery');
  assert.equal(rows.length, 6);
  const locals = await optionLocals(9);
  const options = rows.filter(row => row.platform === 'ios').map(row =>
    runInNewContext('(' + row.expression + ')', locals)).find(row => row.environment.E2E_ROLE === 'receiver');
  assert(options);
  assert.equal(options.testMethod, 'handleRecoveryStep');
  const config = serialize(options.expectedDeviceNames, options.environment);
  assert.equal(config.role, 'receiver'); assert.equal(config.sender, 'web');
  assert.equal(config.cycle, '1'); assert.equal(config.step, '1');
  assert.equal(config.receiverRequestAlreadySent, true);
  assert.deepEqual(plain(options.expectedDeviceNames ?? {}), expectedNames, 'SPECIALIZED_CANONICAL_MAP_REQUIRED Test9 receiver');
});

test('actual specialized stable-member native launch expressions all forward canonical map without role defaulting', async () => {
  let sites = 0;
  for (const [name, number, count] of stableModes) {
    const rows = nativeOptions(name), locals = await optionLocals(number);
    assert.equal(rows.length, count, name + ' original native call-site census');
    for (const row of rows) {
      const options = runInNewContext('(' + row.expression + ')', locals);
      assert.equal(typeof options.testMethod, 'string');
      assert(options.testMethod.length > 0);
      assert.deepEqual(plain(options.expectedDeviceNames ?? {}), expectedNames, name + ' ' + row.platform);
      if (row.platform === 'ios') {
        const config = serialize(options.expectedDeviceNames, options.environment);
        assert.deepEqual([config.expectedWebDeviceName, config.expectedAndroidDeviceName,
          config.expectedIosDeviceName], Object.values(expectedNames));
      }
      sites += 1;
    }
  }
  assert.equal(sites, 20);
});


const phasedModes = [
  ['runWebMultiSecretLifecycle', 22, null],
  ['runAndroidMultiSecretLifecycle', 23, 'Web joined Android vault after direct-show block'],
  ['runIosMultiSecretLifecycle', 24, 'Android joined after the direct-Show block'],
  ['runIosSecondaryCreatorLifecycle', 27, 'Android joined after cycle'],
  ['runWebSecondaryCreatorLifecycle', 25, 'iOS joined after cycle'],
  ['runAndroidSecondaryCreatorLifecycle', 26, 'Web joined after cycle'],
];

async function phaseLocals(number) {
  const locals = await optionLocals(number);
  return { ...locals, approvalPlatform: 'ios',
    verifyBothReceiverAlerts: locals.scenario.recovery.verifyBothReceiverAlerts === true,
    stepTestMethod: 'handleRecoveryStep', secretValueForName: () => 'fixture-value' };
}

test('actual phased recovery metadata uses all twelve launch expressions and preserves explicit iOS factory admission', async () => {
  let count = 0, factoryCount = 0;
  for (const [name, number] of phasedModes) {
    const rows = nativeOptions(name).filter(row => /handleRecoveryStep|stepTestMethod/.test(row.expression));
    assert.equal(rows.length, 2, name + ' recovery-only call-site census');
    const locals = await phaseLocals(number);
    for (const row of rows) {
      const options = runInNewContext('(' + row.expression + ')', locals);
      assert.deepEqual(plain(options.expectedDeviceNames ?? {}), expectedNames, name + ' recovery map');
      if (row.callee === 'startIosAction') {
        factoryCount += 1;
        const text = actualFunction(name), start = text.indexOf('const startIosAction = ') + 'const startIosAction = '.length;
        const end = text.indexOf('\n  });', start) + '\n  });'.length - 1;
        assert(start > 0 && end > start);
        const factory = runInNewContext('(' + text.slice(start, end).trim() + ')', {
          scenario: locals.scenario, simulatorUdid: 'closed-ios', stepEnvironment: createStepEnvironment,
          secretValueForName: () => 'fixture-value', startIosJoinTest: (_udid, value) => value,
        });
        const forwarded = factory(options);
        assert.deepEqual(plain(forwarded.expectedDeviceNames), expectedNames);
        const early = factory({ testMethod: 'showLocalSecretForTest24', role: 'show', secretName: 'fixture-secret' });
        assert.equal(early.expectedDeviceNames, undefined, 'two-member creator/local Show must not receive a future map');
      }
      count += 1;
    }
  }
  assert.equal(count, 12);
  assert.equal(factoryCount, 1, 'actual IosMulti recovery factory branch must execute');
});

test('actual staged discovery statements remain after real third-member boundaries and before recovery closures', () => {
  const declaration = 'const expectedDeviceNames = await discoverCanonicalRecoveryDeviceNames(page);';
  for (const [name, , anchor] of phasedModes.filter(row => row[2])) {
    const text = actualFunction(name), boundary = text.indexOf(anchor), capture = text.indexOf(declaration);
    const recovery = text.indexOf('const startNativeStep = async');
    assert(boundary >= 0 && capture > boundary && recovery > capture, name + ' phase order');
    assert.equal(text.split('await discoverCanonicalRecoveryDeviceNames(page)').length - 1, 1);
  }
  const web = actualFunction('runWebMultiSecretLifecycle');
  const join = web.indexOf('await runWebMultiSecretLifecycleIosJoin(');
  const capture = web.indexOf('expectedDeviceNames = await discoverCanonicalRecoveryDeviceNames(page);');
  assert(web.indexOf('let expectedDeviceNames;') < web.indexOf('const startNativeStep = async'));
  assert(join >= 0 && capture > join && capture < web.indexOf('iOS joined after direct Show cycles='));
  assert.equal(web.split('await discoverCanonicalRecoveryDeviceNames(page)').length - 1, 1);
});

test('actual Test22 recovery closure rejects an unbound two-member phase before launch and accepts the completed-phase map', async () => {
  const text = actualFunction('runWebMultiSecretLifecycle');
  const start = text.indexOf('const startNativeStep = ') + 'const startNativeStep = '.length;
  const end = text.indexOf('\n  };', start) + '\n  }'.length;
  assert(start > 0 && end > start);
  const expression = text.slice(start, end);
  const locals = await phaseLocals(22);
  for (const ready of [false, true]) {
    const attempts = [];
    const launch = (_destination, options) => { attempts.push(options); return { result: Promise.resolve() }; };
    const operation = runInNewContext('(' + expression + ')', { ...locals,
      expectedDeviceNames: ready ? locals.expectedDeviceNames : undefined,
      startIosJoinTest: launch, startAndroidStepTest: launch,
      simulatorUdid: 'closed-ios', androidSerial: 'closed-android' });
    let error;
    try { await operation('ios', 'receiver', { number: 1 }, 'fixture-secret', 'ios', 'web'); }
    catch (caught) { error = caught; }
    if (!ready) { assert(error); assert.equal(attempts.length, 0); }
    else { assert.equal(error, undefined); assert.equal(attempts.length, 1);
      assert.deepEqual(plain(attempts[0].expectedDeviceNames), expectedNames); }
  }
});
