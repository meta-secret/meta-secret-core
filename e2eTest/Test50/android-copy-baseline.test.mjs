import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
const helper = (name) => readFileSync(new URL(`../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/${name}.kt`, import.meta.url), 'utf8');
const privacy = helper('LiveboardPrivacyTestActions');
const authentication = helper('LiveboardAuthenticationTestActions');
const secretActions = helper('LiveboardSecretTestActions');

function assertOrderedAnchors(source, anchors) {
  let previous = -1;
  for (const anchor of anchors) {
    const index = source.indexOf(anchor, previous + 1);
    assert(index >= 0, `Missing required anchor: ${anchor}`);
    assert(index > previous, `Wrong action order: ${anchor}`);
    previous = index;
  }
}
function assertAndroidCancelBranch(actor) {
  const branchStart = actor.indexOf('if (role == "cancel") {');
  assert(branchStart >= 0, 'Missing actual requester/cancel branch');
  const branchEnd = actor.indexOf('composeRule.lbApproveSystemIfPresent()', branchStart);
  assert(branchEnd > branchStart, 'Missing positive-path boundary after cancel return');
  const branch = actor.slice(branchStart, branchEnd);
  assertOrderedAnchors(branch, ['composeRule.lbCancelSystemPrompt()', 'idle(); waitForDeletionRow(name, 30_000)', 'emit("CANCELLED")', 'return']);
  assert(branch.includes('== initialRequests'));
  assert(branch.includes('composeRule.lbValueAbsent(id)'));
  assert(!/orbit-back|orbitTap|performClick|performAction|lbApproveSystemIfPresent|force_system_auth|auto.?success/i.test(branch));
}


test('same-actor Copy baseline is captured after valid Show and before READY/transport hold', () => {
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  assertOrderedAnchors(actor, ['composeRule.lbShow(name, value)', 'copyBounds = captureDeletionCopyBounds()', 'emit("READY")', 'barrier("action")']);
  assert(actor.includes('composeRule.lbAssertHeldReadPrivacy(id, value, checkNotNull(copyBounds))'));
  const capture = privacy.slice(privacy.indexOf('fun ComposeTestRule.lbCaptureCopyBounds('), privacy.indexOf('fun ComposeTestRule.lbAssertHeldReadPrivacy('));
  assert(capture.includes('rootInActiveWindow')); assert(capture.includes('it.isVisibleToUser && it.isEnabled'));
  assert(capture.includes('actionable.size == 1'));
});


test('held-read proof inspects actual foreground redaction and preserves old-point physical clipboard assertions', () => {
  const blocked = privacy.slice(privacy.indexOf('fun ComposeTestRule.lbAssertHeldReadPrivacy('));
  assert(!blocked.includes('automation.windows'));
  assert(blocked.includes('nodes.none')); assert(blocked.includes('contains(secretValue)'));
  assert(blocked.includes('root.packageName?.toString() == instrumentation.targetContext.packageName'));
  assert(blocked.includes('root.isVisibleToUser && nodes.any'));
  assert(blocked.includes('oldCopyBounds.exactCenterX(),') || blocked.includes('oldCopyBounds.exactCenterX();'));
  assert(blocked.includes('oldCopyBounds.exactCenterY()'));
  assert(blocked.includes('uiAutomation.injectInputEvent')); assert(blocked.includes('check(unchanged)'));
  assert.equal((blocked.match(/cache.synchronization.value/g) ?? []).length, 2);
  assert(blocked.includes('assertIsNotEnabled()')); assert(blocked.includes('lbValueAbsent(id)'));
  assert(!blocked.includes('vault-sync-blocker')); assert(!blocked.includes('lbShow('));
});


test('native accessibility is initialized and meaningfully ready before Show; null modal root is never accepted', () => {
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  assertOrderedAnchors(actor, ['deletionAccessibilityRoot(false, false, name)', 'if (role == "show" || role == "sync-observe")']);
  const helper = source.slice(source.indexOf('    private fun prepareDeletionAccessibility()'), source.indexOf('    /** Capture from this actor'));
  assert(helper.includes('FLAG_RETRIEVE_INTERACTIVE_WINDOWS'));
  assert(helper.includes('FLAG_INCLUDE_NOT_IMPORTANT_VIEWS'));
  assert(helper.includes('composeRule.waitUntil(15_000)'));
  assert(helper.includes('it.packageName?.toString() == instrumentation.targetContext.packageName'));
  assert(helper.includes('it.isActive && it.isFocused'));
  assert(helper.includes('ownVisible.isNotEmpty()'));
  assert(helper.includes('anchorText.isNotBlank()'));
  assert(helper.includes('rootPackageMatch=')); assert(helper.includes('visibleOwnDescendants='));
  assert(helper.includes('synchronization.value.let { it.loading || it.activeRefreshes > 0 } && ownVisible.any'));
  assert(helper.includes('info.capabilities'));
  assert(helper.includes('it.isActive')); assert(helper.includes('it.isFocused'));
  assert(helper.includes('checkNotNull(root)'));
});


test('F3 capability probe precedes Vault business and contains no mutation/explicit Core request', () => {
  const probe = source.slice(source.indexOf('    fun verifyTest50AccessibilityReady()'), source.indexOf('    private fun deletionAccessibilityNodes'));
  assert(probe.includes('onboarding-skip')); assert(probe.includes('orbit-enter')); assert(probe.includes('orbit-heading'));
  assert(probe.includes('ANDROID_TEST50_ACCESSIBILITY_READY'));
  assert(!/MetaSecretNative|refreshAppState|createVault|createSecret/.test(probe));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const gate = harness.indexOf("testMethod: 'verifyTest50AccessibilityReady'");
  assert(gate > 0); assert(gate < harness.indexOf('await runDeletionBlock(', gate));
  assert(harness.slice(gate, harness.indexOf('const runCli', gate)).includes("waitForMarker('E2E: ANDROID_TEST50_ACCESSIBILITY_READY'"));
  assert(harness.slice(gate, harness.indexOf('const runCli', gate)).includes("E2E_TEST50_ACCESSIBILITY_PROBE_ONLY === '1'"));
  assert(harness.slice(gate, harness.indexOf('const runCli', gate)).includes('no Vault business executed'));
});


test('F3-only dispatcher stays under parent range guard, validates60 and selects first Android block only', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assertOrderedAnchors(dispatcher, ['validateDeletionPlan(plan)', 'const selectedBlocks']);
  assertOrderedAnchors(dispatcher, ["E2E_RANGE_RUN !== '1'", 'const selectedBlocks']);
  assert(dispatcher.includes("plan.blocks.find((block) => block.members.includes('android'))"));
  assert(dispatcher.includes('selectedBlocks.entries()'));
  assert(dispatcher.includes("F4: 'not-executed'"));
  assert(dispatcher.includes('businessAcceptance: false'));
  assert(dispatcher.includes('businessCyclesExecuted: 0'));
  assert(dispatcher.includes('declaredBusinessCycles: 60'));
  assert(dispatcher.includes("else console.log('✅ Test50: all 8 blocks / 60 cycles complete')"));
});


test('manual complete-tree matching cannot infer privacy from unsupported provider search', () => {
  const helper = source.slice(source.indexOf('    private fun deletionAccessibilityNodes'), source.indexOf('    private fun verifyDeletionReveal'));
  assert(helper.includes('check(nodes.size < 512)'));
  assert(!helper.includes('while (index < nodes.size &&'));
  assert(helper.includes('it.text?.toString() == expected'));
  assert(helper.includes('it.contentDescription?.toString() == expected'));
  assert(helper.includes('it.viewIdResourceName == expected'));
  assert(helper.includes('anchorManualText=')); assert(helper.includes('anchorManualDescription='));
  const operational = helper.split('} catch (failure: Throwable)')[0];
  assert(!operational.includes('findAccessibilityNodeInfosByText'));
  const blocked = helper.slice(helper.indexOf('private fun verifyDeletionOverlayBlocksCopy'));
  assert(!blocked.includes('findAccessibilityNodeInfosByText'));
});


test('probe success awaits shared owned shutdown before exiting to parent restore', () => {
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const branch = harness.slice(harness.indexOf("if (process.env.E2E_TEST50_ACCESSIBILITY_PROBE_ONLY === '1') {"));
  const shutdown = branch.indexOf('await stopProcesses()');
  assert(shutdown >= 0);
  assert(shutdown < branch.indexOf('owned cleanup complete'));
  assert(shutdown < branch.indexOf('process.exit(0)'));
  assert(!branch.slice(0, branch.indexOf('process.exit(0)')).includes('return;'));
});


test('F3 repeat mode is bounded, retains declared60 and uses exact reuse after first fresh child', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assert(dispatcher.includes('probeRepetitions > 5'));
  assert(dispatcher.includes('(!probeOnly && probeRepetitions !== 1)'));
  assert(dispatcher.includes('Array.from({ length: probeRepetitions }'));
  assert(dispatcher.includes("index > 0 ? { E2E_REUSE_PROVEN_ARTIFACTS: '1' }"));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const fresh = harness.indexOf('if (!reuseProvenArtifacts) await recordFreshBuildProvenance()');
  assert(fresh >= 0 && fresh < harness.indexOf("testMethod: 'verifyTest50AccessibilityReady'"));
  const diagnostics = source.slice(source.indexOf('val rootPackage = diagnosticRoot'), source.indexOf('return checkNotNull(root)'));
  assert(diagnostics.includes('rootOwner=$rootOwner'));
  assert(diagnostics.includes('knownPermission=$knownPermission'));
  assert(diagnostics.includes('knownAnr=$knownAnr'));
  assert(diagnostics.includes('knownCrash=$knownCrash'));
  assert(!diagnostics.includes('performClick')); assert(!diagnostics.includes('performAction'));
});


test('failure screenshot is restricted to unauthenticated pre-business F3 and pulled before shutdown', () => {
  const capture = source.slice(source.indexOf('if (argument("role", "") == "accessibility-probe"'), source.indexOf('val info = automation.serviceInfo', source.indexOf('fun classKind')));
  assert(capture.includes('!hasTagNow("lb-secrets") && deletionCachedMember() == null'));
  assert(capture.includes('automation.takeScreenshot()'));
  assert(capture.includes('ANDROID_TEST50_F3_FAILURE_SCREENSHOT_READY'));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const probe = harness.slice(harness.indexOf("testMethod: 'verifyTest50AccessibilityReady'"));
  assertOrderedAnchors(probe, ["'pull'", 'await stopProcesses()']);
  assertOrderedAnchors(probe, ["'pull'", 'throw failure']);
});


test('only F3 can dismiss the screenshot-proved exact16KB system dialog; own foreground remains mandatory', () => {
  const f3 = source.slice(source.indexOf('fun verifyTest50AccessibilityReady'), source.indexOf('private fun deletionAccessibilityNodes'));
  assert(f3.includes('allowKnownCompatibility = true'));
  assert.equal((source.match(/allowKnownCompatibility = true/g) ?? []).length, 1);
  const helper = source.slice(source.indexOf('private fun deletionAccessibilityRoot'), source.indexOf('/** Capture from this actor'));
  assert(helper.includes('allowKnownCompatibility: Boolean = false'));
  assert(helper.includes('candidate?.packageName?.toString() == "android"'));
  assert(helper.includes('This app isn’t 16 KB compatible. ELF alignment check failed.'));
  assert(helper.includes('deletionAccessibilityMatches(nodes, "MetaSecret")'));
  assert(helper.includes('ok.size == 1')); assert(helper.includes('ok.single().performAction'));
  assert(helper.includes('return@waitUntil false'));
  assert(helper.includes('ownWindow && meaningful'));
});


test('Copy text resolves nearest bounded own-window actionable ancestor with containing screen bounds and unique target', () => {
  const capture = privacy.slice(privacy.indexOf('fun ComposeTestRule.lbCaptureCopyBounds('), privacy.indexOf('fun ComposeTestRule.lbAssertHeldReadPrivacy('));
  assert(capture.includes('for (depth in 0..8)')); assert(capture.includes('node.windowId != root.windowId'));
  assert(capture.includes('node.packageName?.toString() != ownPackage'));
  assert(capture.includes('actionBounds.contains(labelBounds)')); assert(capture.includes('candidate = node.parent'));
  assert(capture.includes('}.distinct()')); assert(capture.includes('actionable.size == 1'));
  assert(capture.includes('lb-copy-$id')); assert(capture.includes('check(!labelBounds.isEmpty)'));
});


test('failed cycles13/16/19/23/34 diagnostic is explicit, isolated and cannot claim full matrix acceptance', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assert(dispatcher.includes("!['13', '16', '19', '23', '34'].includes(diagnosticCycle) || probeOnly"));
  assert(dispatcher.includes('cycle.number === Number(diagnosticCycle)'));
  assert(dispatcher.includes('cycles: diagnosticBlock.cycles.filter'));
  assert(dispatcher.includes("stage: 'F4-diagnostic'"));
  assert(dispatcher.includes('NOT full business acceptance'));
  assertOrderedAnchors(dispatcher, ['validateDeletionPlan(plan)', 'const diagnosticCycle']);
});


test('cycle23 focused diagnostic preserves original growth prefix19..23 and reports actual5/60', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assert(dispatcher.includes("diagnosticCycle === '23' ? cycle.number >= 19 && cycle.number <= 23"));
  assert(dispatcher.includes('businessCyclesExecuted: selectedBlocks[0].cycles.length'));
  assert(dispatcher.includes('${selectedBlocks[0].cycles.length}/60 cycles'));
});


test('cycle34 focused prefix preserves original25..34 history/roles with10/60 diagnostic accounting', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assert(dispatcher.includes("diagnosticCycle === '34' ? cycle.number >= 25 && cycle.number <= 34"));
  assert(dispatcher.includes('businessCyclesExecuted: selectedBlocks[0].cycles.length'));
  assert(dispatcher.includes('NOT full business acceptance'));
});

test('Android readiness diagnostics retain ordered actual-entry/target observations and no extra RPC', () => {
  const actor = source.slice(source.indexOf('fun handleSecretDeletionStep'), source.indexOf('/** Test50 growing-list navigation'));
  const checkpoints = [...actor.matchAll(/readiness\("([^"\n]+)"\)/g)].map((match) => match[1]);
  assert.deepEqual(checkpoints, ['actor-enter', 'before-target-row-ready', 'after-target-row-ready']);
  const diagnostic = actor.slice(actor.indexOf('fun readiness(stage:'), actor.indexOf('readiness("actor-enter")'));
  assert.equal((diagnostic.match(/ANDROID_TEST50_READINESS_DIAG/g) ?? []).length, 1);
  assert(diagnostic.includes('currentFrame=')); assert(diagnostic.includes('activeRefreshes='));
  assert(!/MetaSecretNative|refreshAppState|findClaim|getStateModel|getAppState|Thread.sleep/.test(diagnostic));
});

test('real OS authentication is selected before Activity; Cancel never becomes app Cancel or emulator auto-success', () => {
  assert(source.includes('RuleChain')); assert(source.includes('.outerRule(LiveboardRealAuthenticationRule(launchIntent))'));
  assert(source.includes('.around(composeRule)'));
  const actor = source.slice(source.indexOf('fun handleSecretDeletionStep'), source.indexOf('/** Test50 growing-list navigation'));
  assertAndroidCancelBranch(actor);
  assert.throws(() => assertAndroidCancelBranch(actor.replace('composeRule.lbCancelSystemPrompt()', 'composeRule.orbitTap("orbit-back")')));
  assert.throws(() => assertAndroidCancelBranch(actor.replace('composeRule.lbCancelSystemPrompt()', '')));
  assert(authentication.includes('System.setProperty("metasecret.test.force_system_auth", "true")'));
  const cancel = authentication.slice(authentication.indexOf('fun ComposeTestRule.lbCancelSystemPrompt('), authentication.indexOf('fun ComposeTestRule.lbApproveSystemIfPresent('));
  assertOrderedAnchors(cancel, ['systemPrompt()', 'KeyEvent.KEYCODE_BACK']);
  assert(cancel.includes('checkNotNull(root)')); assert(cancel.includes('systemPrompt() == null'));
  assert(!cancel.includes('performClick')); assert(!cancel.includes('orbit-back'));
});

test('controlled Android uses its same prelaunch Intent and returns manual-pending before Delete with real Core unchanged', () => {
  assert(source.includes('ActivityScenarioRule<MainActivity>(launchIntent)'));
  const before = authentication.slice(authentication.indexOf('override fun before()'), authentication.indexOf('override fun after()'));
  assertOrderedAnchors(before, ['profile == "controlled-business"', 'BuildConfig.DEBUG', 'launchIntent.hasExtra("metasecret.force_system_auth")', 'return', 'System.setProperty("metasecret.test.force_system_auth", "true")']);
  assert(before.includes('System.getProperty("metasecret.test.force_system_auth") == null'));
  assert(!before.includes('System.setProperty("metasecret.test.mode"'));
  const actor = source.slice(source.indexOf('fun handleSecretDeletionStep'), source.indexOf('/** Test50 growing-list navigation'));
  const start = actor.indexOf('if (role == "cancel" && argument("authProfile", "real-os") == "controlled-business")');
  const end = actor.indexOf('composeRule.lbTap("lb-delete-$id")', start);
  const branch = actor.slice(start, end);
  assert(start > 0 && end > start); assertOrderedAnchors(branch, ['initialRequests', 'lbValueAbsent(id)', 'emit("AUTH_MANUAL_PENDING")', 'return']);
  assert(!/lbCancelSystemPrompt|CANCELLED|performClick|lbApproveSystemIfPresent/.test(branch));
  assert(source.includes('BuildConfig.META_SECRET_E2E_SERVER_URL == required'));
  assert(source.includes('Os.getenv("METASECRET_E2E_SERVER_URL") == required'));
});

test('successful own refresh verifies SAME selected value after idle with zero replacement Show', () => {
  const actor = source.slice(source.indexOf('fun handleSecretDeletionStep'), source.indexOf('/** Test50 growing-list navigation'));
  const branch = actor.slice(actor.indexOf('"show" -> {'), actor.indexOf('barrier("terminal"); idle()'));
  const refreshAnchors = ['refreshAppState()', 'idle()', 'verifyDeletionReveal(value)', 'lbHide(id)'];
  assertOrderedAnchors(branch, refreshAnchors);
  assert.throws(() => assertOrderedAnchors(branch.replace('refreshAppState()', ''), refreshAnchors));
  const baselineAnchors = ['composeRule.lbShow(name, value)', 'copyBounds = captureDeletionCopyBounds()', 'emit("READY")'];
  assertOrderedAnchors(actor, baselineAnchors);
  assert.throws(() => assertOrderedAnchors(actor.replace('composeRule.lbShow(name, value)', ''), baselineAnchors));
  assert(!branch.includes('lbShow('));
  const verify = source.slice(source.indexOf('private fun verifyDeletionReveal('), source.indexOf('fun createVaultForTest50('));
  assert(verify.includes('lbAssertValue(checkNotNull(composeRule.lbScope().selectedPass), expected)'));
  const hide = secretActions.slice(secretActions.indexOf('fun ComposeTestRule.lbHide('), secretActions.indexOf('fun ComposeTestRule.lbRequestRecovery('));
  assert(hide.includes('lbExactlyOne("lb-value-$id")')); assert(hide.includes('lbConcealed(id)')); assert(!hide.includes('lbShow('));
});

