import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const android = (name) => readFileSync(new URL(`../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/${name}.kt`, import.meta.url), 'utf8');
const ios = (name) => readFileSync(new URL(`../../../meta-secret-compose/iosApp/iosAppUITests/${name}.swift`, import.meta.url), 'utf8');


function assertBefore(source, earlier, later) {
  const first = source.indexOf(earlier), second = source.indexOf(later);
  assert(first >= 0 && second > first, `Required order: ${earlier} before ${later}`);
}

test('Test50 Android creation and positive navigation seek exact IDs in the real semantic lazy list', () => {
  const source = android('CaseEightAndroidBothOfflineTest'), secret = android('LiveboardSecretTestActions'), actions = android('LiveboardTestActions');
  const creation = source.slice(source.indexOf('    private fun createDeletionSecret()'), source.indexOf('    @Test\n    fun approveJoinForTest50()'));
  assert(creation.includes('composeRule.lbCreatePlain(name, value)'));
  assertBefore(creation, 'composeRule.lbCreatePlain(name, value)', 'waitForDeletionRow(name');
  assertBefore(secret, 'lbGone("lb-create"', 'return lbSecret(name)');
  assert(actions.includes('check(matches.size == 1)')); assert(actions.includes('lbNodeTexts(it) == listOf(name)'));
  assert(actions.includes('removePrefix("lb-secret-name-")'));
  const list = source.slice(source.indexOf('    private fun deletionScrollContainer('), source.indexOf('    private fun deletionCachedMember('));
  assert(list.includes('check(containers.size == 1)')); assert(list.includes('onNode(hasScrollToNodeAction(), useUnmergedTree = true)')); assert(list.includes('performScrollToNode(hasTestTag(tag))'));
  const actor = source.slice(source.indexOf('    fun handleSecretDeletionStep()'), source.indexOf('    /** Test50 growing-list navigation'));
  assert(actor.includes('ensureDeletionTag("lb-delete-$id"'));
  assert(actor.includes('assertDeletionTagAbsent("lb-secret-$id")'));
  assert(actor.includes('secrets?.none { it.name == name }'));
});

test('iOS opt-in Test50 creation retains its default and delegates to actual bounded complete-list navigation', () => {
  const source = ios('CaseEightIosBothOfflineUITest'), helper = ios('LiveboardSecretTestActions');
  assert(source.includes('scrollForTest50: Bool = false'));
  const creation = source.slice(source.indexOf('    func createSecretForTest50()'), source.indexOf('    func approveJoinForTest50()'));
  assert(creation.includes('scrollForTest50: true'));
  const delegated = source.slice(source.indexOf('    private func test50FindInList('), source.indexOf('    private func test50Tap('));
  assert(delegated.includes('app.lbFindInList(identifier)'));
  assert(helper.includes('lbTraverseSecrets(target: nil)')); assert(helper.includes('XCTAssertEqual(ids.count, 1'));
  assert(helper.includes('lbFindInList("lb-secret-\\(id)")'));
  assert(helper.includes('XCTAssertTrue(reachedBothEnds'));
});
