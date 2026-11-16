import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const workspace = new URL('../../../', import.meta.url);
const androidRoot = new URL('meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/', workspace);
const iosRoot = new URL('meta-secret-compose/iosApp/iosAppUITests/', workspace);
const read = (root, name) => readFileSync(new URL(name, root), 'utf8');
const android = readdirSync(androidRoot).filter((name) => name.endsWith('.kt'));
const ios = readdirSync(iosRoot).filter((name) => name.endsWith('.swift'));
const androidHelper = read(androidRoot, 'OrbitRegistrationTestActions.kt');
const iosHelper = read(iosRoot, 'CaseOneJoinFromIosUITest.swift').split('@MainActor\nfinal class')[0];


function assertBefore(source, earlier, later) {
  const first = source.indexOf(earlier), second = source.indexOf(later);
  assert(first >= 0 && second > first, `Required order: ${earlier} before ${later}`);
}

test('all native actors use Orbit selectors instead of legacy registration controls', () => {
  const legacy = /"(?:signin-email-manual|email-input|manual-signin-continue|email-confirmation-continue|email-confirmation-join(?:ing)?|email-confirmation-error(?:-reset)?)"/;
  for (const [root, names] of [[androidRoot, android], [iosRoot, ios]]) {
    for (const name of names) assert(!legacy.test(read(root, name)), `Legacy registration selector in ${name}`);
  }
});

test('shared signup actions wait for transition readiness and canonical Pending', () => {
  for (const source of [androidHelper, iosHelper]) {
    assert(source.includes('orbit-transition-ready'));
    assert(source.includes('orbit-phase-idle'));
    assert(source.includes('orbit-phase-pending'));
    assertBefore(source, 'orbit-request', 'orbit-phase-pending');
    assert(!/Thread\.sleep|usleep\(|RunLoop\.current\.run|Task\.sleep/.test(source));
  }
  assert(androidHelper.includes('orbitHasTag("onboarding-skip") || orbitHasTag("orbit-enter")'));
  assert(iosHelper.includes('skip.exists || enter.exists'));
});

test('JOIN_REQUEST_SENT producers follow the shared canonical Pending barrier', () => {
  let producers = 0;
  for (const [root, names] of [[androidRoot, android], [iosRoot, ios]]) {
    for (const name of names) {
      const sections = read(root, name).split(/^    (?:(?:private|internal) )?(?:fun|func) /m).slice(1);
      for (const section of sections) {
        const marker = section.search(/(?:marker\("|print\("E2E: )[^"\n]*JOIN_REQUEST_SENT/);
        if (marker < 0) continue;
        producers++;
        assert(section.indexOf('.orbitRequestJoin()') >= 0 && section.indexOf('.orbitRequestJoin()') < marker,
          `JOIN_REQUEST_SENT has no canonical Pending barrier in ${name}: ${section.split('\n')[0]}`);
      }
    }
  }
  assert(producers > 0);
});

test('selected Test1–3 native actor methods remain addressable', () => {
  for (const [root, file, method] of [
    [androidRoot, 'CaseOneJoinFromAndroidTest.kt', 'joinApprovedVaultAndWaitForRecoveryCompletion'],
    [androidRoot, 'CaseThreeAndroidInitiatorTest.kt', 'createVaultThenApproveWebAndIos'],
    [iosRoot, 'CaseOneJoinFromIosUITest.swift', 'testJoinApprovedVaultAndShowSecret'],
    [iosRoot, 'CaseTwoIosInitiatorUITest.swift', 'testCreateVaultThenApproveAndroidAndWeb'],
    [iosRoot, 'CaseThreeIosJoinUITest.swift', 'testJoinAndroidInitiatedVaultAndHandleRecovery'],
  ]) assert(read(root, file).includes(`${method}()`), `Missing native actor ${file}/${method}`);
});

test('fourth-device error and retry retain explicit separate Core/UI outcomes', () => {
  for (const [root, file] of [[androidRoot, 'CaseEightAndroidBothOfflineTest.kt'], [iosRoot, 'CaseEightIosBothOfflineUITest.swift']]) {
    const source = read(root, file);
    const start = source.indexOf('rejectFourthDeviceAndRegisterNewVault()');
    const body = source.slice(start, source.indexOf('\n    }', start));
    assert(body.includes('awaitPending = false') || body.includes('awaitPending: false'));
    assert(body.includes('orbit-phase-error'));
    assert(body.includes('orbit-back'));
    assertBefore(body, 'FOURTH_DEVICE_REJECTED', 'orbit-back');
    assertBefore(body, 'orbitCreateVault()', 'FOURTH_DEVICE_RETRY_REGISTERED');
  }
});


test('registration Created readiness waits for the real Liveboard, never a fabricated legacy destination', () => {
  const androidCreate = androidHelper.slice(androidHelper.indexOf('>.orbitCreateVault()'), androidHelper.indexOf('>.orbitRequestJoin('));
  const iosCreate = iosHelper.slice(iosHelper.indexOf('func orbitCreateVault()'), iosHelper.indexOf('func orbitRequestJoin('));
  assertBefore(androidCreate, 'orbitTap("orbit-create")', 'lbWaitMain(180_000)');
  assert(androidCreate.includes('orbitWaitForTag("lb-home", 180_000)'));
  assertBefore(iosCreate, 'orbitTap("orbit-create")', 'orbitWaitForTag("lb-home", timeout: 180)');
  for (const source of [androidCreate, iosCreate]) assert(!/emit|marker|print\(|main-tab|tab-secrets/.test(source));
});

test('shared native concealed-state contracts reject ambiguous duplicate nodes as well as a visible value', () => {
  const actions = read(androidRoot, 'LiveboardTestActions.kt');
  assert(actions.includes('lbAbsent(tag: String): Boolean = lbCount(tag) == 0'));
  assert(actions.includes('check(count <= 1)')); assert(actions.includes('count == 1')); assert(actions.includes('lbHas(tag: String): Boolean = lbCount(tag) > 0'));
  assert(actions.includes('lbAbsent("lb-value-$id") && lbAbsent("lb-copy-$id")'));
  const secret = read(androidRoot, 'LiveboardSecretTestActions.kt');
  const hide = secret.slice(secret.indexOf('fun ComposeTestRule.lbHide('), secret.indexOf('fun ComposeTestRule.lbRequestRecovery('));
  assert(hide.includes('lbExactlyOne("lb-value-$id")')); assert(hide.includes('lbConcealed(id)'));
  assert(!hide.includes('lbShow('));
});
