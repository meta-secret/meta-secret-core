import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildDeletionPlan, validateDeletionPlan, resolveDeletionAuthProfile, deletionCoverage, validateDeletionResult, requireDeletionResumeProfile, deletionEntryComplete } from './plan.mjs';
import { deletionMarkerLedger } from './markers.mjs';
import { execFileSync } from 'node:child_process';
import { runInNewContext } from 'node:vm';
import { EventEmitter } from 'node:events';
import { resolve, dirname } from 'node:path';
import { createHash } from 'node:crypto';

test('modified executable coordinators parse without importing or executing their owner entry', () => {
  for (const path of [new URL('../Test4/orchestrator.mjs', import.meta.url), new URL('./orchestrator.mjs', import.meta.url)]) {
    execFileSync(process.execPath, ['--check', path.pathname], { stdio: 'pipe', timeout: 5000 });
  }
});

test('actual profile preflight preserves default realOS and refuses host conflicts without mutation ports', () => {
  assert.equal(resolveDeletionAuthProfile(), 'real-os');
  for (const profile of ['real-os', 'controlled-business']) {
    assert.equal(resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: profile }, { authProfile: profile }), profile);
    for (const key of ['METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE']) for (const value of ['', 'false', 'true']) {
      assert.throws(() => resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: profile, [key]: value }));
      assert.throws(() => resolveDeletionAuthProfile({}, { authProfile: profile, nested: { [key]: value } }));
    }
  }
  for (const value of ['', null, false, 'automatic']) assert.throws(() => resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: value }));
  assert.throws(() => resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: 'controlled-business' }, { authProfile: 'real-os' }));
  for (const key of ['E2E_ANDROID_DEVICE_PASSCODE', 'E2E_IOS_DEVICE_PASSCODE', 'devicePasscode', 'metasecret.force_system_auth', 'metasecret.test.force_system_auth']) {
    assert.throws(() => resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: 'controlled-business', [key]: 'synthetic-not-a-credential' }));
  }
  for (const key of ['JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS']) {
    assert.throws(() => resolveDeletionAuthProfile({ E2E_TEST50_AUTH_PROFILE: 'controlled-business', [key]: '-Dmetasecret.test.force_system_auth=true' }));
  }
  assert.equal(resolveDeletionAuthProfile({ E2E_ANDROID_DEVICE_PASSCODE: 'synthetic-owned-input' }), 'real-os');
});

test('actual profile receipt validator and resume reject partial stale failed or cross-profile acceptance', () => {
  const authProfile = 'controlled-business', runId = 'profile-receipt', attempt = 2;
  const blocks = buildDeletionPlan(runId).blocks.map(block => ({ id: block.id, cycles: block.cycles.map(cycle => cycle.number), configurationSha256: 'a'.repeat(64) }));
  const expected = { runId, attempt, authProfile, blocks };
  const good = { version: 1, ...expected, stage: 'F4', business: { status: 'passed', blocks: 8, cycles: 60 },
    outcome: { code: 0, signal: null, cleanupErrors: [] }, ...deletionCoverage(authProfile) };
  assert.equal(validateDeletionResult(good, expected), good);
  const result = { test: 50, attempt, authProfile, status: 'business-passed-manual-pending', acceptance: good };
  assert.equal(deletionEntryComplete(result, authProfile), true);
  assert.equal(deletionEntryComplete(result, 'real-os'), false);
  for (const mutate of [value => value.attempt++, value => value.authProfile = 'real-os', value => value.stage = 'F3',
    value => value.business.cycles--, value => value.blocks.pop(), value => value.blocks[0].cycles.reverse(),
    value => value.outcome.code = 1, value => value.outcome.signal = 'SIGTERM', value => value.outcome.cleanupErrors.push('owned cleanup failed'),
    value => value.manualOs.status = 'passed', value => value.fullAcceptance = true]) {
    const bad = structuredClone(good); mutate(bad);
    assert.throws(() => validateDeletionResult(bad, expected));
    assert.equal(deletionEntryComplete({ ...result, acceptance: bad }, authProfile), false);
  }
  assert.equal(requireDeletionResumeProfile(undefined, 'real-os'), 'real-os');
  assert.equal(requireDeletionResumeProfile(authProfile, authProfile), authProfile);
  assert.throws(() => requireDeletionResumeProfile(undefined, authProfile));
  assert.throws(() => requireDeletionResumeProfile(authProfile, 'real-os'));
  assert.equal(deletionEntryComplete({ test: 1, status: 'passed' }, authProfile), true);
});

test('controlled ledger retains all actors roles and621 events with exactly36 manual-pending substitutions', () => {
  const plan = buildDeletionPlan('profile-ledger'), original = deletionMarkerLedger(plan), controlled = deletionMarkerLedger(plan, 'controlled-business');
  assert.deepEqual([controlled.produced, controlled.consumed, controlled.diagnostics], [621, 541, 80]);
  assert.equal(controlled.ledger.filter(entry => entry.marker.includes('_AUTH_MANUAL_PENDING_')).length, 36);
  assert.equal(controlled.ledger.filter(entry => entry.marker.includes('_CANCELLED_')).length, 0);
  assert.equal(controlled.ledger.filter(entry => entry.producer === 'ios.setUpWithError').length, 88);
  assert.deepEqual(controlled.ledger.map(entry => entry.marker.replace('_AUTH_MANUAL_PENDING_', '_CANCELLED_')), original.ledger.map(entry => entry.marker));
  assert.deepEqual(validateDeletionPlan(plan).cycles, 60);
});

test('full matrix covers every platform policy and role exactly three repetitions per variant', () => {
  const plan = buildDeletionPlan('f1');
  assert.equal(validateDeletionPlan(plan).cycles, 60);
  for (const block of plan.blocks) for (const [index] of block.variants.entries()) {
    assert.deepEqual(block.cycles.filter((cycle) => cycle.variant === index + 1).map((cycle) => cycle.repetition), [1, 2, 3]);
  }
  const broken = structuredClone(plan); broken.blocks.pop();
  assert.throws(() => validateDeletionPlan(broken));
});

test('marker ledger has one scoped producer and explicit consumer for every action', () => {
  const { ledger, produced, consumed, diagnostics } = deletionMarkerLedger(buildDeletionPlan('f1'));
  assert.equal(produced, 621); assert.equal(consumed, 541); assert.equal(diagnostics, 80);
  assert.equal(new Set(ledger.map(({ key }) => key)).size, produced);
  assert(ledger.every(({ producer, consumer }) => producer && consumer));
  const passive = ledger.filter(({ producer }) => producer.includes('cachedAppState'));
  assert(passive.every(({ consumer }) => consumer.includes('diagnostic') || consumer.includes('identity metadata')));
});

test('all orchestrated native class/method names exist and agreed marker producers match', () => {
  const android = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
  const ios = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  for (const [source, keyword, className] of [[android, 'fun', 'CaseEightAndroidBothOfflineTest'], [ios, 'func', 'CaseEightIosBothOfflineUITest']]) {
    assert(source.includes(`class ${className}`));
    for (const method of ['handleSecretDeletionStep', 'createVaultForTest50', 'createSecretForTest50']) assert(new RegExp(`${keyword} ${method}\\(`).test(source), `Missing ${className}.${method}`);
  }
  assert(android.includes('ANDROID_TEST50_SECRET_CREATED_'));
  assert(ios.includes('IOS_TEST50_SECRET_CREATED_'));
  assert(ios.includes('let expectRequest: Bool?'));
});


test('native deletion dispatch forwards selected real classes and exact scoped role, subject, value and outcome', () => {
  const business = readFileSync(new URL('./business.mjs', import.meta.url), 'utf8');
  const dispatch = business.slice(business.indexOf('const native = async ('), business.indexOf('const wait = (actor'));
  assert(dispatch.includes("testClass: scenario.android.testClass, testMethod: 'handleSecretDeletionStep'"));
  assert(dispatch.includes("testClass: scenario.ios.testClass, testMethod: 'handleSecretDeletionStep'"));
  assert(dispatch.includes('role, cycle: cycle.number, step: scopeStep(step), secretName: cycle.secretName'));
  assert(dispatch.includes('secretValue: h.secretValue(cycle.secretName)'));
  assert(dispatch.includes('E2E_ROLE: role, E2E_CYCLE: String(cycle.number), E2E_STEP: scopeStep(step)'));
  assert(dispatch.includes('E2E_EXPECTED_OUTCOME: cycle.outcome'));
  assert(dispatch.includes('E2E_EXPECT_REQUEST: String(block.members.length === 3)'));
  const owned = dispatch.indexOf('actors.add(actor); h.onNativeActor(actor)');
  const ready = dispatch.indexOf("marker(platform, 'READY'");
  assert(owned >= 0 && ready > owned, 'Owned actor failure binding must exist before READY');
});

test('Swift held-read protections use the whole original foreground window and physically probe its original Copy point', () => {
  const root = new URL('../../../meta-secret-compose/iosApp/iosAppUITests/', import.meta.url);
  const privacy = readFileSync(new URL('LiveboardPrivacyTestActions.swift', root), 'utf8');
  const guard = privacy.slice(privacy.indexOf('func lbAssertReadGuard('), privacy.indexOf('func lbProbeOldCopyPoint('));
  assert(guard.includes('XCTAssertEqual(state, .runningForeground'));
  assert(guard.includes('XCTAssertEqual(window, original'));
  assert(guard.includes('window.contains(rect)'));
  assert(guard.includes('descendants(matching: .any).allElementsBoundByIndex'));
  assert(guard.includes('element.label.contains(expected)'));
  assert(guard.includes('(element.value as? String)?.contains(expected)'));
  assert(guard.includes('"lb-value-"')); assert(guard.includes('"lb-copy-"'));
  assert(guard.includes('XCTAssertTrue(control.exists')); assert(guard.includes('XCTAssertFalse(control.isEnabled'));
  const probe = privacy.slice(privacy.indexOf('func lbProbeOldCopyPoint('), privacy.indexOf('func lbAssertNoPrivateContent('));
  assert.equal((probe.match(/lbAssertReadGuard\(expected/g) ?? []).length, 2);
  assert(probe.includes('UIPasteboard.general.string = sentinel'));
  assert(probe.includes('old.midX - frame.minX')); assert(probe.includes('old.midY - frame.minY'));
  assert(probe.includes('UIPasteboard.general.string == sentinel')); assert(probe.includes('defer { UIPasteboard.general.string = before }'));
  assert(!probe.includes('lbShow('));
  const actor = readFileSync(new URL('CaseEightIosBothOfflineUITest.swift', root), 'utf8');
  function assertWiredPrivacy(text) {
    const start = text.indexOf('func handleSecretDeletionStep()');
    const end = text.indexOf('private func test50RectNumbers(', start);
    assert(start >= 0 && end > start, 'Actual actor boundary is missing');
    const body = text.slice(start, end);
    const coldStart = body.indexOf('if role == "startup-gone" {');
    const coldEnd = body.indexOf('enterSimulatorPasscodeIfNeeded()', coldStart);
    assert(coldStart >= 0 && coldEnd > coldStart);
    const cold = body.slice(coldStart, coldEnd);
    const firstProbe = cold.indexOf('app.lbProbeOldCopyPoint(value, baseline: baselineGeometry)');
    const ready = cold.indexOf('marker("READY")');
    const action = cold.indexOf('gate("action")');
    const secondProbe = cold.indexOf('app.lbProbeOldCopyPoint(value, baseline: baselineGeometry)', firstProbe + 1);
    const terminal = cold.indexOf('gate("terminal")');
    assert(firstProbe >= 0 && ready > firstProbe && action > ready && secondProbe > action && terminal > secondProbe,
      'Cold-start actor must physically probe before READY and again during the held read before terminal release');
    const syncStart = body.indexOf('case "sync-observe":');
    const syncEnd = body.indexOf('default:', syncStart);
    assert(syncStart >= 0 && syncEnd > syncStart);
    const sync = body.slice(syncStart, syncEnd);
    const blocked = sync.indexOf('test50SyncGeometry(phase: "blocked", baseline: baselineGeometry)');
    const physical = sync.indexOf('app.lbProbeOldCopyPoint(value, baseline: baselineGeometry, id: id)');
    const reported = sync.indexOf('marker("SYNC_BLOCKED")');
    const released = sync.indexOf('gate("sync-release")');
    assert(blocked >= 0 && physical > blocked && reported > physical && released > reported,
      'Sync observer must prove actual held privacy before releasing the network hold');
    assert(!/orbitTap|lbShow\(/.test(sync));
  }
  assertWiredPrivacy(actor);
  assert.throws(() => assertWiredPrivacy(actor.replaceAll('app.lbProbeOldCopyPoint(value, baseline: baselineGeometry)', '')));
  assert.throws(() => assertWiredPrivacy(actor.replace('app.lbProbeOldCopyPoint(value, baseline: baselineGeometry, id: id)', '')));

});

// Execute the exact named phase consumed by its real owner, without importing
// executable owner top-level code or granting SDK/device/process ports.
function owningPhase(path, name, ports) {
  const source = readFileSync(new URL(path, import.meta.url), 'utf8');
  const start = source.indexOf(`async function ${name}(`), end = source.indexOf('\n}\n', start);
  assert(start >= 0 && end > start, `Actual owning phase missing: ${name}`);
  const call = { performCleanup: 'cleanup: performCleanup,', finishDeletionBlock: 'await finishDeletionBlock();',
    runDeletionBlocks: 'await runDeletionBlocks();', finishRangeOwnership: 'await finishRangeOwnership({' }[name];
  assert(call && source.includes(call), `Real caller must consume ${name}`);
  return runInNewContext(`(${source.slice(start, end + 2)})`, { AggregateError, console: { log() {}, error() {} }, ...ports });
}

test('actual Test4 deletion cleanup settles every owned attempt and propagates failures in both profiles', async () => {
  for (const profile of ['real-os', 'controlled-business']) for (const actorsFail of [true, false]) {
    const calls = [], failure = name => async () => {
      calls.push(name);
      if (actorsFail || !['actors', 'restore'].includes(name)) throw Error(`${name} cleanup failed`);
    };
    const cleanup = owningPhase('../Test4/orchestrator.mjs', 'performCleanup', {
      scenario: { mode: 'vault-secret-deletion' }, deletionAuthProfile: profile,
      terminateOwnedProcessTree: failure('actors'), browserInstance: { close: failure('browser') },
      networkGateProcesses: new Map([['one', { close: failure('gate1') }], ['two', { close: failure('gate2') }]]),
      approvalCoordinator: { close: failure('coordinator') }, exitOnSuccess: true, composeRoot: '/fixture/compose',
      runAndWait: async file => failure(file === 'docker' ? 'server' : 'gradle')(),
      serverContainerOwned: true, serverContainer: 'owned-fixture', restoreEnvironmentGuard: failure('restore'),
    });
    let observed; try { await cleanup(); } catch (error) { observed = error; }
    assert.deepEqual(calls, ['actors', 'browser', 'gate1', 'gate2', 'coordinator', 'gradle', 'server', 'restore']);
    assert(observed instanceof AggregateError); assert.equal(observed.errors.length, actorsFail ? 8 : 6);
    const failed = calls.filter(name => actorsFail || !['actors', 'restore'].includes(name));
    assert.deepEqual(observed.errors.map(error => error.message), failed.map(name => `${name} cleanup failed`));
  }
  const calls = [];
  const legacy = owningPhase('../Test4/orchestrator.mjs', 'performCleanup', {
    scenario: { mode: 'unchanged-other-mode' }, terminateOwnedProcessTree: async () => calls.push('actors'),
    browserInstance: { close: async () => { calls.push('browser'); throw Error('legacy soft failure'); } },
    networkGateProcesses: new Map(), approvalCoordinator: undefined, exitOnSuccess: false,
    serverContainerOwned: false, restoreEnvironmentGuard: async () => calls.push('restore'),
  });
  await legacy(); assert.deepEqual(calls, ['actors', 'browser', 'restore']);
});

test('actual Test4 deletion receipt cannot follow a signal racing its joined shutdown', async () => {
  for (const profile of ['real-os', 'controlled-business']) {
    const writes = [], exits = [], ports = { scenario: { deletionBlock: buildDeletionPlan('receipt-signal').blocks[0] },
      scenarioPath: '/fixture/scenario.json', deletionAuthProfile: profile, requestedSignalExitCode: undefined,
      process: { env: { E2E_RUN_ID: 'receipt-signal' }, exit: code => exits.push(code) },
      resolve, dirname, createHash, deletionCoverage, readFileSync: () => Buffer.from('{}'), writeFileSync: (...args) => writes.push(args),
    };
    // The actual phase and stop port share this mutable VM global, as the real
    // signal handler and main continuation share requestedSignalExitCode.
    const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
    const start = source.indexOf('async function finishDeletionBlock('), end = source.indexOf('\n}\n', start);
    const operation = runInNewContext(`globalThis.stopProcesses = async () => { requestedSignalExitCode = 143; }; (${source.slice(start, end + 2)})()`, ports);
    await assert.rejects(operation, /cancel|interrupt|signal/i);
    assert.equal(writes.length, 0); assert.deepEqual(exits, []);
    const success = owningPhase('../Test4/orchestrator.mjs', 'finishDeletionBlock', { ...ports,
      requestedSignalExitCode: undefined, stopProcesses: async () => {} });
    await success(); assert.equal(writes.length, 1); assert.deepEqual(exits, [0]);
    writes.length = 0; exits.length = 0;
    const failed = owningPhase('../Test4/orchestrator.mjs', 'finishDeletionBlock', { ...ports,
      stopProcesses: async () => { throw Error('actual cleanup rejected'); } });
    await assert.rejects(failed(), /cleanup rejected/); assert.equal(writes.length, 0); assert.deepEqual(exits, []);
  }
});

test('actual Test50 parent fences racing child0 before another block or aggregate in both profiles', async () => {
  for (const profile of ['real-os', 'controlled-business']) for (const timing of ['before-first', 'during-first', 'during-last', 'during-failed', 'none']) {
    const runId = 'parent-signal', selectedBlocks = buildDeletionPlan(runId).blocks, files = new Map(), signals = {}, calls = [];
    const processPort = { execPath: '/fixture/node', env: {}, once: (name, callback) => {
      signals[name] = callback; if (timing === 'before-first' && name === 'SIGTERM') callback();
    } };
    let spawns = 0, closed = 0;
    const phase = owningPhase('./orchestrator.mjs', 'runDeletionBlocks', {
      root: '/fixture/Test50', runId, authProfile: profile, attempt: 1, selectedBlocks, base: {},
      configDirectory: '/fixture/config', diagnosticCycle: undefined, probeOnly: false, process: processPort,
      resolve, createHash, deletionCoverage, validateDeletionResult, readFileSync: path => files.get(path),
      writeFileSync: (path, value) => { files.set(path, value); calls.push(path); },
      spawn: (_file, args) => {
        assert.equal(closed, spawns, 'Next child must wait for prior close, not only exit');
        const block = selectedBlocks[spawns++], child = new EventEmitter();
        child.kill = () => { calls.push('kill'); if (timing === 'during-failed') throw Error('owned cancellation failed'); return true; };
        const configurationSha256 = createHash('sha256').update(files.get(args[1])).digest('hex');
        const receipt = { version: 1, runId, attempt: 1, authProfile: profile, stage: 'F4',
          blocks: [{ id: block.id, cycles: block.cycles.map(cycle => cycle.number), configurationSha256 }],
          business: { status: 'passed', blocks: 1, cycles: block.cycles.length }, outcome: { code: 0, signal: null, cleanupErrors: [] },
          ...deletionCoverage(profile, block.cycles.filter(cycle => ['android', 'ios'].includes(cycle.requester)).length) };
        files.set(resolve('/fixture/config', `block-${block.id}-attempt-1.result.json`), JSON.stringify(receipt));
        queueMicrotask(() => {
          if ((['during-first', 'during-failed'].includes(timing) && spawns === 1) || (timing === 'during-last' && spawns === selectedBlocks.length)) signals.SIGTERM();
          const code = timing === 'during-failed' ? 1 : 0;
          child.emit('exit', code, null);
          queueMicrotask(() => { closed++; child.emit('close', code, null); });
        });
        return child;
      },
    });
    if (timing === 'none') {
      await phase(); assert.equal(spawns, 8); assert.equal(closed, 8);
      assert(calls.some(path => path.endsWith('result-attempt-1.json'))); continue;
    }
    let observed; try { await phase(); } catch (error) { observed = error; }
    assert.match(observed?.message ?? '', /cancel|interrupt|signal/i);
    assert.equal(spawns, timing === 'before-first' ? 0 : timing === 'during-last' ? 8 : 1);
    assert.equal(closed, spawns);
    assert(!calls.some(path => path.endsWith('result-attempt-1.json')), 'Interrupted aggregate must not be written');
    if (timing === 'during-failed') {
      assert(observed.errors.some(error => /failed code=1/.test(error.message)));
      assert(observed.errors.some(error => /owned cancellation failed/.test(error.message)));
      assert.match(observed.cause.message, /failed code=1/);
    }
  }
});

test('actual range ownership finalizer refuses Test50 own-runtime stop failure and still restores/closes', async () => {
  for (const profile of ['real-os', 'controlled-business']) for (const otherCleanupFails of [false, true]) {
    const calls = [], primary = Error('original scenario failed'), runtimeFailure = Error('owned existing runtime did not stop');
    const manifest = { status: profile === 'real-os' ? 'completed' : 'business-completed-manual-pending',
      runtimeOwnership: [{ platform: 'iOS', startedByRun: true, udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30' }] };
    const phase = owningPhase('../run-ui-e2e-range.mjs', 'finishRangeOwnership', {
      stopRunnerOwnedRuntimes: async records => { assert.equal(records, manifest.runtimeOwnership); calls.push('stop'); throw runtimeFailure; },
      releaseHealthyRunnerOwnedRuntimes: async () => { calls.push('release'); assert.fail('Unexpected release in strict Test50 cleanup fixture'); },
      writeManifest: () => calls.push('manifest'), process: { stdout: { write() {} }, stderr: { write() {} } },
    });
    let observed; try { await phase({ manifest, manifestPath: '/fixture/manifest', strictDeletion: true, primaryFailure: primary,
      collector: { writeEvent() {}, close: async () => { calls.push('close'); if (otherCleanupFails) throw Error('collector close failed'); } },
      environmentGuard: { restore: async () => { calls.push('restore'); if (otherCleanupFails) throw Error('restore failed'); } } });
    } catch (error) { observed = error; }
    assert(!calls.includes('release'), 'Strict Test50 cleanup must not release owned runtimes');
    assert(observed, 'Owned Test50 stop failure cannot return successful completion');
    assert.equal(manifest.status, otherCleanupFails ? 'restore-failed' : 'owned-runtime-cleanup-failed'); assert.equal(manifest.evidencePreserved, true);
    assert.equal(manifest.fullAcceptance, false); assert(calls.indexOf('restore') > calls.indexOf('stop')); assert.equal(calls.at(-1), 'close');
    assert.equal(observed.cause, primary); assert(observed.errors.includes(primary)); assert(observed.errors.includes(runtimeFailure));
    assert.equal(observed.errors.length, otherCleanupFails ? 4 : 2);
  }
  const calls = [], manifest = { status: 'completed', runtimeOwnership: [{ startedByRun: true }] };
  const legacy = owningPhase('../run-ui-e2e-range.mjs', 'finishRangeOwnership', {
    stopRunnerOwnedRuntimes: async () => { calls.push('stop'); throw Error('legacy warning'); },
    releaseHealthyRunnerOwnedRuntimes: async () => { calls.push('release'); assert.fail('Unexpected release in legacy Test50 cleanup fixture'); },
    writeManifest() {}, process: { stdout: { write() {} }, stderr: { write() {} } },
  });
  await legacy({ manifest, strictDeletion: false, environmentGuard: { restore: async () => calls.push('restore') },
    collector: { writeEvent() {}, close: async () => calls.push('close') } });
  assert.equal(manifest.status, 'completed'); assert.deepEqual(calls, ['stop', 'restore', 'close']);
});
