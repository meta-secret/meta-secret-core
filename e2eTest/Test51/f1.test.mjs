import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { buildRegistrationPlan, validateRegistrationPlan, registrationMarkers, caseBlockers, platforms, providers, outcomes } from './plan.mjs';
import { webName } from './web.mjs';

const source = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const plan = buildRegistrationPlan('f1');

test('expanded matrix retains9Create/27Join/9QR variants, one repetition and no unrelated Secrets', () => {
  assert.deepEqual(validateRegistrationPlan(plan), { cases: 45, create: 9, join: 27, qr: 9, markers: 479, repetitions: 1 });
  for (const platform of platforms) for (const provider of providers) {
    assert.equal(plan.cases.filter((entry) => entry.kind === 'create' && entry.platform === platform && entry.provider === provider).length, 1);
    assert.deepEqual(plan.cases.filter((entry) => entry.kind === 'join' && entry.platform === platform && entry.provider === provider).map(({ outcome }) => outcome), outcomes);
  }
  const broken = structuredClone(plan); broken.cases.pop();
  assert.throws(() => validateRegistrationPlan(broken));
  const repeated = structuredClone(plan); repeated.repetitions = 3;
  assert.throws(() => validateRegistrationPlan(repeated));
  const scenario = JSON.parse(source('./scenario.json'));
  assert.deepEqual(scenario.secrets, {});
  assert.deepEqual(scenario.recovery.cyclePlan, []);
});

test('each independent case has unique synthetic DB/identity boundary and mixed-platform roles', () => {
  assert.equal(new Set(plan.cases.flatMap(({ vaultName, retryVaultName }) => [vaultName, retryVaultName])).size, 90);
  for (const entry of plan.cases) {
    assert.equal(entry.cleanBoundary, 'fresh-server-container-and-app-sandboxes-and-browser-context');
    assert.equal(new Set([entry.platform, entry.approver, entry.retryApprover]).size, 3);
  }
});

test('synthetic Vault emails remain <=40 characters for arbitrarily long valid run IDs', () => {
  for (const runId of ['f1', 'registration_decline_20261018135952_r3', 'long_'.repeat(200)]) {
    const sample = buildRegistrationPlan(runId);
    for (const name of sample.cases.flatMap(({ vaultName, retryVaultName }) => [vaultName, retryVaultName])) {
      assert(name.length <= 40, `Synthetic fixture exceeds the40-character contract: ${name.length}`);
      assert.match(name, /^[a-zA-Z0-9._+-]+@e2e\.invalid$/);
    }
  }
});

test('fixture generation is deterministic and separates full run ID, case and primary/retry stage', () => {
  const prefix = 'same_long_prefix_'.repeat(40);
  const runIds = [`${prefix}a`, `${prefix}b`, 'another-run'];
  const names = [];
  for (const runId of runIds) {
    const sample = buildRegistrationPlan(runId);
    assert.deepEqual(sample, buildRegistrationPlan(runId), 'No random/date-based fixture drift');
    for (const entry of sample.cases) {
      assert.notEqual(entry.vaultName, entry.retryVaultName, 'Retry uses a different canonical Vault');
      names.push(entry.vaultName, entry.retryVaultName);
    }
  }
  assert.equal(names.length, 270);
  assert.equal(new Set(names).size, names.length, 'Full run suffix, case and stage must all contribute to identity');
});

test('short synthetic names preserve full run scope, exact case identities and the479 marker ledger', () => {
  const runId = 'registration_decline_20261018135952_long_scope_unchanged';
  const sample = buildRegistrationPlan(runId);
  assert.equal(sample.runId, runId);
  assert.deepEqual(sample.cases.map(({ id }) => id), plan.cases.map(({ id }) => id));
  const markers = sample.cases.flatMap((entry) => registrationMarkers(sample, entry));
  assert.equal(markers.length, 479);
  assert(markers.every(({ key }) => key.startsWith(`E2E: TEST51_${runId}_`)));
  assert(sample.cases.every(({ cleanBoundary }) => cleanBoundary === 'fresh-server-container-and-app-sandboxes-and-browser-context'));
});

test('checked-in case inventory matches the executable registration plan', () => {
  assert.deepEqual(JSON.parse(source('./cases.json')), buildRegistrationPlan('registration'));
});

test('marker ledger declares one unique producer and consumer per completed assertion', () => {
  const markers = plan.cases.flatMap((entry) => registrationMarkers(plan, entry));
  assert.equal(new Set(markers.map(({ key }) => key)).size, 479);
  assert(markers.every(({ producer, consumer }) => producer && consumer));
  assert(markers.every(({ key }) => key.includes('TEST51_f1_')));
  for (const entry of plan.cases.filter(({ outcome }) => outcome === 'back-retry')) {
    const actions = registrationMarkers(plan, entry);
    assert(!actions.some(({ platform, action }) => platform === entry.approver && action === 'DECIDED'), 'Back occurs before any primary decision');
    assert(actions.some(({ platform, action }) => platform === entry.platform && action === 'RESET01'));
    assert.equal(actions.some(({ action }) => action === 'RESET_UI01'), entry.platform === 'ios');
    assert(!actions.some(({ action }) => action === 'BACK02'));
  }
  for (const entry of plan.cases.filter(({ outcome }) => outcome === 'declined-retry')) {
    const actions = registrationMarkers(plan, entry);
    assert(actions.some(({ platform, action }) => platform === entry.platform && action === 'RESET01'));
    assert.equal(actions.some(({ action }) => action === 'RESET_UI01'), entry.platform === 'ios');
    assert(!actions.some(({ action }) => action === 'BACK02'));
  }
});

test('provider prerequisites remain explicit blockers, never manual/mock substitutes', () => {
  const available = plan.cases.filter((entry) => caseBlockers(entry, {}).length === 0);
  assert.equal(available.length, 12);
  assert.equal(available.flatMap((entry) => registrationMarkers(plan, entry)).length, 119);
  const appleAndroid = plan.cases.find(({ id }) => id === 'create-android-apple');
  assert(caseBlockers(appleAndroid, { E2E_REAL_ANDROID_APPLE_READY: '1', E2E_REAL_ANDROID_APPLE_EMAIL: 'fixture@example.com' }).some((reason) => reason.includes('unsupported')));
  const webGoogle = plan.cases.find(({ id }) => id === 'create-web-google');
  assert(caseBlockers(webGoogle, { E2E_REAL_WEB_GOOGLE_READY: '1' }).some((reason) => reason.includes('exact account email')));
  assert.equal(caseBlockers(webGoogle, { E2E_REAL_WEB_GOOGLE_READY: '1', E2E_REAL_WEB_GOOGLE_EMAIL: 'fixture@example.com' }).length, 0);
  assert(plan.cases.filter(({ kind }) => kind === 'qr').every((entry) => caseBlockers(entry, {}).some((reason) => reason.includes('physical-device'))));
  assert(source('./orchestrator.mjs').includes('vaultName: planned.provider === \'manual\' ? planned.vaultName : providerEmail'));
});

test('native actor selectors and consumed primary postconditions exist in exact owning sources', () => {
  const android = source('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/Case51AndroidRegistrationTest.kt');
  const ios = source('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift');
  assert(android.includes('class Case51AndroidRegistrationTest'));
  assert(ios.includes('class CaseEightIosBothOfflineUITest'));
  assert(android.includes('fun handleRegistrationForTest51()'));
  assert(ios.includes('func handleRegistrationForTest51()'));
  for (const action of ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED', 'TAKEN', 'PENDING', 'DECLINED', 'DECIDED']) {
    assert(android.includes(`"${action}"`), `Android producer missing ${action}`);
    assert(ios.includes(`"${action}"`), `iOS producer missing ${action}`);
  }
  assert(android.includes('RESET01'));
  assert(ios.includes('cleanup-presence-verified'), 'Both iOS unfinished outcomes must wait for the positive host oracle before Back');
  assert(ios.includes('RESET_UI01') && ios.includes('reset-verified'), 'iOS UI reset cannot certify cleanup before the exact host oracle');
});

test('native Case51 availability helpers assert inverse Join enablement before completed markers', () => {
  const android = source('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/Case51AndroidRegistrationTest.kt');
  const ios = source('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift');
  const androidName = android.slice(android.indexOf('private fun enterName('), android.indexOf('private fun requester('));
  const iosName = ios.slice(ios.indexOf('private func registration51EnterName('), ios.indexOf('private func registration51Requester('));
  assert(androidName.includes('onNodeWithTag("orbit-join", useUnmergedTree = true)'));
  assert(androidName.includes('if (available) join.assertIsNotEnabled() else join.assertIsEnabled()'));
  assert(iosName.includes('["orbit-join"].isEnabled, !available'));
});

test('Web availability postconditions reject incorrect Join state before AVAILABLE or TAKEN is emitted', async () => {
  // Unit-only page double; no browser/account is launched or credited as business coverage.
  const name = 'availability@e2e.invalid';
  function fixture(available, joinEnabled) {
    const markers = [];
    const controls = {
      first() { return this; },
      getByRole(_role, { name: label } = {}) {
        return { isEnabled: async () => label === 'Create' ? available : label === 'Join' ? joinEnabled : true,
          click: async () => {}, innerText: async () => name };
      },
      getAttribute: async () => available ? 'available' : 'exists',
      waitFor: async () => {},
    };
    const page = { locator: () => controls, getByRole: () => controls };
    return { page, actor: { provider: 'apple', vaultName: name, actor: available ? 'create' : 'requester' }, markers };
  }
  for (const available of [true, false]) {
    const correct = fixture(available, !available);
    await webName(correct.page, correct.actor, (marker) => correct.markers.push(marker));
    assert.deepEqual(correct.markers, [available ? 'AVAILABLE' : 'TAKEN']);
    const wrong = fixture(available, available);
    await assert.rejects(webName(wrong.page, wrong.actor, (marker) => wrong.markers.push(marker)), /Join/);
    assert.deepEqual(wrong.markers, [], 'An incorrect Join state cannot be certified by a completed marker');
  }
});

test('Web only case avoids native preflight/build/boot and uses requested URL port', () => {
  const harness = source('../Test4/orchestrator.mjs');
  assert(harness.includes("registrationOnly && registrationPlatforms.every((platform) => platform === 'web')"));
  assert(harness.includes("!registrationOnly || registrationPlatforms.includes('ios')"));
  assert(harness.indexOf('await runRegistrationCase(') < harness.indexOf('// Clean the iOS simulator before Android starts.'));
  assert(harness.includes("'--port', webEndpoint.port || '5173', '--strictPort'"));
  const branch = harness.slice(harness.indexOf('if (registrationOnly) {\n    let simulatorUdid'), harness.indexOf('// Clean the iOS simulator before Android starts.'));
  assert(!branch.includes('createWebSecret('));
  assert(branch.includes("registrationPlatforms.includes('android')"));
});

test('all new Node programs and shared lifecycle parse before starting any service', () => {
  for (const path of ['./orchestrator.mjs', './business.mjs', './web.mjs', './plan.mjs', '../Test4/orchestrator.mjs', '../run-ui-e2e-range.mjs']) {
    const result = spawnSync(process.execPath, ['--check', fileURLToPath(new URL(path, import.meta.url))], { encoding: 'utf8' });
    assert.equal(result.status, 0, `${path}: ${result.stderr}`);
  }
});

test('both incomplete outcomes use exact read-only before and after cleanup postconditions', () => {
  const web = source('./web.mjs');
  assert(web.includes('await proveLocalReset(incompleteAttempt)'));
  assert(web.includes('await captureIncompleteIdentity(actor.vaultName, actor.outcome'));
  assert(web.includes('await assertWebRegistration01(page)'));
  for (const label of ['Use Apple email', 'Use Google email', 'Enter your email']) assert(web.includes(label));
  assert(!/page\.(reload|goto)\(/.test(web), 'Requester stays online through decisions/retry');
  const business = source('./business.mjs');
  assert(web.includes('readRegistrationSnapshot(state)'));
  assert(web.includes("if (!state) throw new Error"));
  assert(web.includes("expectedKind === 'pending' || expectedKind === 'declined'"));
  assert(web.includes('{ kind: expectedKind, name }'));
  assert(web.includes('proof.deviceId, incompleteAttempt.deviceId'));
  assert(web.includes("name === 'meta-secret' ? 1 : 0"));
  assert(web.includes('proof.databases[name].user, 0'));
  assert(web.includes("{ kind: 'local', name: '' }"));
  assert(web.includes("database.transaction(['commit_log'], 'readonly')"));
  assert(web.includes("['DeviceCreds:index::', 'UserCreds:index::']"));
  assert(web.includes('store.count(IDBKeyRange.bound'));
  assert(!web.includes('.getAll(') && !web.includes('.getAllKeys('), 'Key values and unrelated record lists never leave IndexedDB');
  assert(!business.includes('proveLocalReset: undefined'));
  assert(source('../Test4/orchestrator.mjs').includes("registrationOnly ? { name: '', value: '', environment }"), 'Registration native actors cannot require a fabricated Secret fixture');
});

test('accepted platform roles run before reset variants without reducing the requested matrix', () => {
  const joins = plan.cases.filter(({ kind }) => kind === 'join');
  assert(joins.slice(0, 9).every(({ outcome }) => outcome === 'accepted'));
  assert(joins.slice(9, 18).every(({ outcome }) => outcome === 'declined-retry'));
  assert(joins.slice(18).every(({ outcome }) => outcome === 'back-retry'));
  assert(plan.cases.slice(0, 9).every(({ kind }) => kind === 'create'));
  assert(plan.cases.slice(36).every(({ kind }) => kind === 'qr'));
});

// Core client_view filters Declined outsiders; responder success must not require a hidden terminal row.
test('native Decline postconditions correlate one candidate and keep a visible receiving device', () => {
  const android = source('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/Case51AndroidRegistrationTest.kt');
  const ios = source('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift');
  const androidResponder = android.slice(android.indexOf('private fun responder('), android.indexOf('private fun assertGeneratedQr('));
  const iosResponder = ios.slice(ios.indexOf('private func registration51Responder('), ios.indexOf('private func registration51GeneratedQr('));
  assert(!androidResponder.includes('else "Declined"'), 'Declined outsiders are filtered out of Core client_view');
  assert(!iosResponder.includes('accepted ? "Member" : "Declined"'));
  assert(androidResponder.includes('JoinRequestAlertState.Visible'));
  assert(androidResponder.includes('getUserDataByDeviceId(candidateId) == null'));
  assert(androidResponder.includes('device-row-$currentId'));
  const decline = androidResponder.slice(androidResponder.indexOf('// Core client_view'));
  assert(decline.includes('onAllNodes(hasTestTag("device-row-$candidateId"), useUnmergedTree = true).fetchSemanticsNodes().isEmpty()'));
  assert(decline.includes('onAllNodes(hasTestTag("pending-device-row"), useUnmergedTree = true).fetchSemanticsNodes().isEmpty()'));
  assert(!decline.includes('!hasTag('), 'Duplicate matches must never certify candidate absence');
  assert(iosResponder.includes('pendingRows.count == 1'));
  assert(iosResponder.includes('currentRows.count == 1'));
  assert(iosResponder.includes('let visibleRows = self.registration51VisibleDeviceRows()'));
  assert(iosResponder.includes('visibleRows.contains'));
  assert(iosResponder.includes('currentRowIdentifier'));
  assert(iosResponder.includes('exactDeviceRowIdentifier(name: name, status: "Member")'));
  assert(source('./business.mjs').includes("await wait(requester, entry.platform, 'DECLINED')"));
});
