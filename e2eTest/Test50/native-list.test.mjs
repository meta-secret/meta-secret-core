import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('Test50 Android creation and positive navigation seek exact tags in the semantic lazy list', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseEightAndroidBothOfflineTest.kt', import.meta.url), 'utf8');
  const creation = source.slice(source.indexOf('    private fun createDeletionSecret()'), source.indexOf('    @Test\n    fun approveJoinForTest50()'));
  assert(creation.indexOf('waitForTagGone("secret-name-input"') < creation.indexOf('waitForDeletionRow(name'));
  assert(source.includes('onNode(hasScrollToNodeAction(), useUnmergedTree = true)'));
  assert(source.includes('performScrollToNode(hasTestTag(tag))'));
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  assert(actor.includes('ensureDeletionTag("open-deletion-request-$name"'));
  assert(actor.includes('ensureDeletionTag("delete-secret-$name"'));
  assert(actor.includes('assertDeletionTagAbsent("secret-row-$name")'));
  assert(actor.includes('secrets?.none { it.name == name }'));
});

test('iOS uses an opt-in Test50 list helper and legacy creation defaults remain unchanged', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  assert(source.includes('app.scrollViews'));
  assert(source.includes('scrollForTest50: Bool = false'));
  const creation = source.slice(source.indexOf('    func createSecretForTest50()'), source.indexOf('    func approveJoinForTest50()'));
  assert(creation.includes('scrollForTest50: true'));
});
