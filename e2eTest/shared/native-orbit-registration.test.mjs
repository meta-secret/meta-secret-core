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
    assert(source.indexOf('orbit-phase-pending') > source.indexOf('orbit-request'));
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
    assert(body.indexOf('FOURTH_DEVICE_REJECTED') < body.indexOf('orbit-back'));
    assert(body.indexOf('orbitCreateVault()') < body.indexOf('FOURTH_DEVICE_RETRY_REGISTERED'));
  }
});
