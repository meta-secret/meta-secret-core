import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');

test('same-actor Copy baseline is captured after valid Show and before READY/transport hold', () => {
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  const baseline = actor.indexOf('baselineCopyBounds = captureDeletionCopyBounds()');
  assert(baseline > actor.indexOf('verifyDeletionReveal(value)'));
  const ready = actor.indexOf('emit("READY")', baseline); assert(ready > baseline);
  assert(actor.indexOf('barrier("action")', ready) > ready);
  assert(actor.includes('verifyDeletionOverlayBlocksCopy(value, checkNotNull(baselineCopyBounds)'));
  const capture = source.slice(source.indexOf('    private fun captureDeletionCopyBounds()'), source.indexOf('    /** Physical touch uses'));
  assert(capture.includes('deletionAccessibilityRoot(copyBaseline = true)'));
  assert(capture.includes('it.isVisibleToUser && it.isEnabled'));
  assert(capture.includes('actionable.size == 1'));
});

test('modal proof never queries hidden background Copy and retains foreground/physical clipboard assertions', () => {
  const blocked = source.slice(source.indexOf('    private fun verifyDeletionOverlayBlocksCopy('), source.indexOf('    private fun verifyDeletionReveal('));
  assert(!blocked.includes('automation.windows'));
  assert(blocked.includes('foregroundNodes.none'));
  assert(blocked.includes('contains(secretValue)'));
  assert(blocked.includes('deletionAccessibilityMatches(foregroundNodes, copyLabel).isEmpty()'));
  assert(blocked.includes('bounds.exactCenterX(), bounds.exactCenterY()'));
  assert(blocked.includes('automation.injectInputEvent'));
  assert(blocked.includes('check(unchanged)'));
  assert(blocked.includes('check(hasTagNow("vault-sync-blocker"))'));
});

test('native accessibility is initialized and meaningfully ready before Show; null modal root is never accepted', () => {
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  assert(actor.indexOf('deletionAccessibilityRoot(copyBaseline = false, blockerRequired = false, anchorText = name)') < actor.indexOf('if (role == "show" || role == "sync-observe")'));
  const helper = source.slice(source.indexOf('    private fun prepareDeletionAccessibility()'), source.indexOf('    /** Capture from this actor'));
  assert(helper.includes('FLAG_RETRIEVE_INTERACTIVE_WINDOWS'));
  assert(helper.includes('FLAG_INCLUDE_NOT_IMPORTANT_VIEWS'));
  assert(helper.includes('composeRule.waitUntil(15_000)'));
  assert(helper.includes('it.packageName?.toString() == instrumentation.targetContext.packageName'));
  assert(helper.includes('it.isActive && it.isFocused'));
  assert(helper.includes('ownVisible.isNotEmpty()'));
  assert(helper.includes('anchorText.isNotBlank()'));
  assert(helper.includes('rootPackageMatch=')); assert(helper.includes('visibleOwnDescendants='));
  assert(helper.includes('hasTagNow("vault-sync-blocker") && blockerLabels.any'));
  assert(helper.includes('info.capabilities'));
  assert(helper.includes('it.isActive')); assert(helper.includes('it.isFocused'));
  assert(helper.includes('checkNotNull(root)'));
});


test('F3 capability probe precedes Vault business and contains no mutation/explicit Core request', () => {
  const probe = source.slice(source.indexOf('    fun verifyTest50AccessibilityReady()'), source.indexOf('    private fun deletionAccessibilityNodes'));
  assert(probe.includes('onboarding-skip')); assert(probe.includes('signin-email-manual'));
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
  assert(dispatcher.indexOf('validateDeletionPlan(plan)') < dispatcher.indexOf('const selectedBlocks'));
  assert(dispatcher.indexOf("E2E_RANGE_RUN !== '1'") < dispatcher.indexOf('const selectedBlocks'));
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
  assert(capture.includes('!hasTagNow("tab-secrets") && deletionCachedMember() == null'));
  assert(capture.includes('automation.takeScreenshot()'));
  assert(capture.includes('ANDROID_TEST50_F3_FAILURE_SCREENSHOT_READY'));
  const harness = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const probe = harness.slice(harness.indexOf("testMethod: 'verifyTest50AccessibilityReady'"));
  assert(probe.indexOf("'pull'") < probe.indexOf('await stopProcesses()'));
  assert(probe.indexOf("'pull'") < probe.indexOf('throw failure'));
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
  const capture = source.slice(source.indexOf('private fun captureDeletionCopyBounds'), source.indexOf('/** Physical touch uses'));
  assert(capture.includes('for (depth in 0..8)'));
  assert(capture.includes('node.windowId != foreground.windowId'));
  assert(capture.includes('node.packageName?.toString() != targetPackage'));
  assert(capture.includes('actionBounds.contains(labelBounds)'));
  assert(capture.includes('candidate = node.parent'));
  assert(capture.includes('}.distinct()'));
  assert(capture.includes('actionable.size == 1'));
  assert(capture.includes('actionDepths='));
});


test('failed cycles13/16/19/23/34 diagnostic is explicit, isolated and cannot claim full matrix acceptance', () => {
  const dispatcher = readFileSync(new URL('./orchestrator.mjs', import.meta.url), 'utf8');
  assert(dispatcher.includes("!['13', '16', '19', '23', '34'].includes(diagnosticCycle) || probeOnly"));
  assert(dispatcher.includes('cycle.number === Number(diagnosticCycle)'));
  assert(dispatcher.includes('cycles: diagnosticBlock.cycles.filter'));
  assert(dispatcher.includes("stage: 'F4-diagnostic'"));
  assert(dispatcher.includes('NOT full business acceptance'));
  assert(dispatcher.indexOf('validateDeletionPlan(plan)') < dispatcher.indexOf('const diagnosticCycle'));
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

test('Android readiness diagnostics have12 ordered checkpoints, paired bounded metadata phases and no extra RPC', () => {
  const actor = source.slice(source.indexOf('fun handleSecretDeletionStep'), source.indexOf('/** Test50 growing-list navigation'));
  const checkpoints = [...actor.matchAll(/readiness\("([^"\n]+)"\)/g)].map((match) => match[1]);
  assert.deepEqual(checkpoints, ['before-tab-visible', 'after-tab-visible', 'before-tab-click', 'after-tab-click', 'before-loader-gone', 'after-loader-gone', 'before-target-row-ready', 'after-target-row-ready', 'before-canonical-target-present', 'after-canonical-target-present', 'before-id-marker', 'after-id-marker']);
  const diagnostic = actor.slice(actor.indexOf('fun readiness(stage:'), actor.indexOf('if (role == "startup-gone")'));
  assert.equal((diagnostic.match(/ANDROID_TEST50_READINESS_DIAG/g) ?? []).length, 2);
  assert(diagnostic.includes('phase=enter')); assert(diagnostic.includes('phase=ui'));
  assert(diagnostic.includes('SystemClock.elapsedRealtime() - readinessStarted'));
  assert(diagnostic.includes('canonicalTargetPresent='));
  assert(!/MetaSecretNative|refreshAppState|findClaim|getStateModel|getAppState|Thread.sleep/.test(diagnostic));
  assert(diagnostic.indexOf('phase=enter') < diagnostic.indexOf('val tabVisible'));
});
