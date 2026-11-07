import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { readFileSync, writeFileSync, mkdtempSync, rmSync, existsSync, mkdirSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { buildCreateInputFixture } from './ios-create-input-fixture.mjs';
import { buildCreateCallerFixture } from './ios-create-caller-fixture.mjs';

let root = resolve(import.meta.dirname);
while (!existsSync(join(root, 'meta-secret-compose/iosApp/iosAppUITests/LiveboardCreateTestActions.swift'))) {
  const parent = dirname(root); assert.notEqual(parent, root, 'Workspace source root required'); root = parent;
}
const sourcePath = process.env.CREATE_ACK_SOURCE_PATH ?? join(root,
  'meta-secret-compose/iosApp/iosAppUITests/LiveboardCreateTestActions.swift');
let rows, callerRows, directory;
before(() => {
  const sources = buildCreateInputFixture(readFileSync(sourcePath, 'utf8'));
  directory = process.env.CREATE_ACK_OUTPUT_DIR ?? mkdtempSync(join(tmpdir(), 'create-ack-bank-'));
  for (const [name, body] of Object.entries(sources)) {
    assert(body.split('\n').length <= 400, `Generated ${name} source exceeds400`);
    writeFileSync(join(directory, name === 'main' ? 'main.swift' : `${name}.swift`), body);
  }
  const executable = join(directory, 'create-ack-fixture');
  execFileSync('/usr/bin/xcrun', ['swiftc', join(directory, 'ports.swift'), join(directory, 'owning.swift'),
    join(directory, 'main.swift'), '-o', executable], { encoding: 'utf8', timeout: 60_000 });
  rows = JSON.parse(execFileSync(executable, [], { encoding: 'utf8', timeout: 10_000 }));
  const swiftRoot = join(root, "meta-secret-compose/iosApp/iosAppUITests");
  const callerSources = buildCreateCallerFixture(readFileSync(sourcePath, "utf8"),
    readFileSync(process.env.CREATE_ACK_CASE_TWO_PATH ?? join(swiftRoot, "CaseTwoIosInitiatorUITest.swift"), "utf8"),
    readFileSync(process.env.CREATE_ACK_CASE_FOUR_PATH ?? join(swiftRoot, "CaseFourIosConcurrentRecoveryUITest.swift"), "utf8"));
  const callerDirectory = join(directory, "callers"); mkdirSync(callerDirectory);
  for (const [name, body] of Object.entries(callerSources)) {
    assert(body.split("\n").length <= 400, `Generated caller ${name} source exceeds400`);
    writeFileSync(join(callerDirectory, `${name}.swift`), body);
  }
  const callerExecutable = join(callerDirectory, "create-callers");
  execFileSync("/usr/bin/xcrun", ["swiftc", ...["ports", "owning", "callers", "main"].map(n => join(callerDirectory, `${n}.swift`)),
    "-o", callerExecutable], { encoding: "utf8", timeout: 60_000 });
  callerRows = JSON.parse(execFileSync(callerExecutable, [], { encoding: "utf8", timeout: 10_000 }));
});
after(() => { if (directory && !process.env.CREATE_ACK_OUTPUT_DIR) rmSync(directory, { recursive: true, force: true }); });
const row = name => { const value = rows.find(r => r.mode === name); assert(value, name); return value; };
function originalSuccess(name) {
  const actual = row(name);
  assert.equal(actual.failures, 0, `${name}: original assertions`);
  assert.equal(actual.finalExact, true, `${name}: original exact equality`);
  assert.equal(actual.inputSequenceExact, true, `${name}: original grapheme order`);
  assert.equal(actual.attempted, actual.requested, `${name}: submitted once`);
  assert.equal(actual.submits, 0, `${name}: replacement does not Submit`);
  return actual;
}
function acknowledged(name) {
  const actual = originalSuccess(name);
  assert.equal(actual.applicationCalls, 0, 'Input must address exact field');
  assert.equal(actual.fieldCalls, Math.ceil(actual.requested / 3) + actual.deletions, 'One call per max3 chunk; deletion remains single');
  actual.inputCallSizes.forEach((size, index) => {
    const deletes = actual.deleteCallSizes[index], call = index + 1;
    assert((size >= 1 && size <= 3 && deletes === 0) || (size === 0 && deletes === 1), 'Actual call contains max3 new Characters or ONE delete');
    const sent = actual.events.indexOf('call:' + call + ':end');
    const ack = actual.events.indexOf('call-ack:' + call + ':exact', sent + 1);
    const next = actual.events.indexOf('call:' + (call + 1) + ':start', sent + 1);
    assert(sent >= 0 && ack > sent && (next < 0 || ack < next), 'Fresh exact whole-call prefix before NEXT native call');
  });
  assert.equal(actual.returnedBool, true);
  return actual;
}

test('actual Create original no-loss geometry and exact text controls', () => {
  for (const mode of ['plain', 'coordinate', 'old-text', 'placeholder', 'unicode', 'value', 'word', 'empty', 'reflow']) originalSuccess(mode);
  assert.equal(row('coordinate').coordinateTaps, 1);
  assert.equal(row('plain').coordinateTaps, 0);
  assert.equal(row('old-text').deletions, 4);
  assert.equal(row('placeholder').deletions, 0);
});

test('actual Create original tail precedes strict targeted prefix ACK contract', () => {
  originalSuccess('plain');
  acknowledged('plain');
});

test('actual Create bulk transcript precedes strict first-loss stop contract', () => {
  const actual = row('drop-first-five');
  assert(actual.failures > 0, 'Original exact assertion must refuse dropped input');
  assert.equal(actual.finalExact, false);
  assert.equal(actual.submits, 0);
  if (actual.applicationCalls > 0) {
    assert.equal(actual.attempted, 13, 'Old complete bulk input transcript');
    assert.equal(actual.observedCharacters, 8, 'Old fault exposes suffix8');
    assert.equal(actual.inputSequenceExact, true, 'Fault does not alter submitted input');
  }
  assert.equal(actual.attempted, 3, 'First unacknowledged max3 call must stop NEXT chunk');
  assert.equal(actual.fieldCalls, 1, 'Exactly one targeted submission, never retry');
  assert.equal(actual.applicationCalls, 0);
  assert.equal(actual.returnedBool, false);
});

test('actual Create acknowledges unchanged original Unicode value seed and clearing inputs', () => {
  for (const mode of ['coordinate', 'old-text', 'placeholder', 'unicode', 'value', 'word', 'empty', 'reflow']) acknowledged(mode);
});

test('actual Create refuses middle loss transformation duplicate and wrong recipient before NEXT', () => {
  for (const [mode, limit] of [['drop-second', 3], ['transform-second', 3], ['duplicate-second', 3], ['wrong-recipient', 3]]) {
    const actual = row(mode);
    assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, limit, mode);
    assert.equal(actual.returnedBool, false, mode);
    assert.equal(actual.submits, 0, mode);
  }
});

test('actual Create shares finite budget and fresh scope across acknowledgements', () => {
  for (const mode of ['scope-after-input', 'deadline-after-first']) {
    const actual = row(mode);
    assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, 3, mode);
    assert.equal(actual.returnedBool, false, mode);
  }
  assert(row('deadline-after-first').elapsed <= 61, 'Port advances61s; approved shared60 cannot renew');
});

test('actual Create can observe pending rendering without resubmitting input', () => { acknowledged('pending-render'); });

test('actual Create original geometry scope and immutable traversal refusals perform no input', () => {
  for (const mode of ['missing', 'duplicate', 'disabled', 'wrong-type', 'nonfinite', 'offclip', 'modal-disabled',
    'viewport-outside', 'keyboard-covered', 'snapshot-error', 'node-budget', 'tag-budget', 'reflow-invalid']) {
    const actual = row(mode);
    assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, 0, mode);
    assert.equal(actual.applicationCalls + actual.fieldCalls, 0, mode);
    assert.equal(actual.submits, 0, mode);
  }
});

test('actual Create caller happy bodies retain ordinary Submit and readiness', () => {
  for (const actual of callerRows.filter(r => r.mode === 'plain')) {
    assert.equal(actual.failures, 0, actual.caller);
    if (actual.caller !== 'shared-fill') assert.equal(actual.submits, 1, actual.caller);
    if (!actual.caller.startsWith('shared-')) assert.equal(actual.ready, true, actual.caller);
  }
});

test('actual Create Boolean refusals stop next field Submit added and readiness even with nonthrowing XCT ports', () => {
  for (const actual of callerRows.filter(r => r.mode === 'drop-first-five')) {
    assert(actual.failures > 0, actual.caller);
    assert.equal(actual.attempted, 3, actual.caller);
    assert.equal(actual.valueInputs, 0, actual.caller);
    assert.equal(actual.submits, 0, actual.caller);
    assert.equal(actual.finds, 0, actual.caller);
    assert.equal(actual.ready, false, actual.caller);
    assert.equal(actual.added, false, actual.caller);
  }
});

test("actual Create late AX getters cannot authorize input after shared deadline", () => {
  for (const mode of ["late-hittable", "late-value"]) {
    const actual = row(mode); assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, 0, mode); assert.equal(actual.taps, mode === "late-hittable" ? 0 : 1, mode);
    assert.equal(actual.returnedBool, false, mode);
  }
  const lateAck = row("late-ack-value"); assert.equal(lateAck.attempted, 3); assert.equal(lateAck.returnedBool, false);
});

test("actual Create literal placeholder remains legitimate requested text", () => { acknowledged("literal-placeholder"); });

test("actual Create clearing rollback and replacement refuse without healing", () => {
  const clearing = row("clear-noop"); assert(clearing.failures > 0); assert.equal(clearing.attempted, 0);
  assert.equal(clearing.deletions, 1); assert.equal(clearing.returnedBool, false);
  for (const [mode, count] of [["rollback-second", 3], ["replace-after-input", 3]]) {
    const actual = row(mode); assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, count, mode); assert.equal(actual.returnedBool, false, mode);
  }
});

test('actual Create same live-accessor cost preserves exact once-only input within original shared phase', () => {
  originalSuccess('plain');
  const actual = row('native-ax-cost');
  assert.equal(actual.requested, 13);
  assert(actual.liveReads > 0, 'Actual owning live accessors ran');
  assert.equal(actual.inputPrefixExact, true, 'No loss/transformation injected by cost port');
  assert.equal(actual.applicationCalls, 0, 'Current targeted input baseline');
  assert.equal(actual.finalExact, true, 'Observed accessor cost must fit unchanged30s without dropping input');
  assert.equal(actual.failures, 0);
  assert.equal(actual.attempted, 13);
  assert.equal(actual.returnedBool, true);
  assert(actual.elapsed <= 30);
});

test('actual Create immutable snapshot scope refuses unknown malformed and foreign ownership before input', () => {
  for (const mode of ['snap-error', 'snap-offclip', 'snap-wrong-type', 'snap-disabled',
    'snap-missing-field', 'snap-duplicate-field', 'snap-missing-modal', 'snap-duplicate-modal',
    'snap-missing-viewport', 'snap-duplicate-viewport', 'snap-no-window', 'snap-foreign-field',
    'snap-foreign-viewport', 'snap-keyboard-covered', 'snap-late', 'snap-nonfinite-field', 'snap-nonfinite-keyboard',
    'snap-modal-outside', 'snap-viewport-outside', 'snap-root-nonfinite', 'snap-window-nonfinite']) {
    const actual = row(mode);
    assert(actual.snapshots >= 1, mode);
    assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, 0, mode);
    assert.equal(actual.returnedBool, false, mode);
    assert.equal(actual.submits, 0, mode);
  }
});

test('actual Create 20k immutable frontier refuses before expansion and performs no input', () => {
  const actual = row('snap-wide-frontier');
  assert.equal(actual.snapshots, 1);
  assert.equal(actual.childrenReads, 1, 'Root frontier read exactly once before bounded refusal');
  assert(actual.failures > 0);
  assert.equal(actual.attempted, 0);
  assert.equal(actual.returnedBool, false);
});

test('actual Create snapshot refusal preserves original no-input gates before one bounded diagnostic', () => {
  const actual = row('snap-foreign-field');
  assert(actual.failures > 0, 'Original actual snapshot refusal remains');
  assert.equal(actual.returnedBool, false);
  assert.equal(actual.attempted, 0);
  assert.equal(actual.submits, 0);
  assert.equal(actual.snapshots, 1, 'No extra snapshot used to manufacture diagnostic');
  assert.equal(actual.liveReads, 91, 'Root prior actual-source transcript retained');
  assert.equal(actual.taps, 1, 'Only original focus tap');
  assert.equal(actual.applicationCalls + actual.fieldCalls, 0, 'No ordinary input attempt');
  assert.equal(actual.diagnostics.length, 1, 'One existing refusal must expose fixed metadata');
  assert.match(actual.diagnostics[0], /^E2E: CREATE_INPUT_REFUSAL phase=PRE_SEND reason=field-window-mismatch /);
});

const refusalReasons = new Set(['unsupported-field', 'snapshot-error', 'root-frame-invalid', 'node-budget',
  'window-frame-invalid', 'required-node-frame-invalid', 'required-node-disabled', 'keyboard-frame-invalid',
  'frontier-budget', 'required-cardinality', 'required-node-unavailable', 'modal-no-window', 'window-not-found',
  'viewport-window-mismatch', 'field-window-mismatch', 'field-type', 'root-modal-containment',
  'window-modal-containment', 'modal-viewport-containment', 'clip-invalid', 'field-outside-clip',
  'live-value-nil', 'live-value-other', 'post-live-value-deadline', 'pre-observation-deadline', 'post-observation-deadline',
  'prefix-mismatch', 'unexpected-prefix', 'bounded-unacknowledged']);
function diagnostic(actual) {
  assert.equal(actual.diagnostics.length, 1, actual.mode);
  const line = actual.diagnostics[0];
  assert(Buffer.byteLength(line) <= 512, actual.mode);
  const parsed = /^E2E: CREATE_INPUT_REFUSAL phase=(PRE_SEND|ACK|FINAL) reason=([a-z-]+) shape=(.*)$/.exec(line);
  assert(parsed, actual.mode); assert(refusalReasons.has(parsed[2]), parsed[2]);
  if (parsed[3] !== 'UNAVAILABLE') {
    const shape = /^scanned=(T|F),visited=(\d+),queued=(\d+),windows=(\d+),keyboards=(\d+),modal=(ZERO|ONE|MANY),viewport=(ZERO|ONE|MANY),field=(ZERO|ONE|MANY)$/.exec(parsed[3]);
    assert(shape, actual.mode);
    for (const count of shape.slice(2, 6)) assert(Number(count) <= 8192, actual.mode);
  }
  assert(!/CANARY|fixture-alpha|secondary|Placeholder/.test(line), 'No field text or dynamic description');
  return { line, phase: parsed[1], reason: parsed[2], shape: parsed[3] };
}

test('actual Create fixed diagnostic schema distinguishes partial nil other deadline and mismatch without extra input', () => {
  const expected = new Map([
    ['snap-error', 'snapshot-error'], ['snap-wide-frontier', 'frontier-budget'],
    ['snap-foreign-field', 'field-window-mismatch'], ['live-nil-pre', 'live-value-nil'],
    ['live-other-pre', 'live-value-other'], ['live-wrong-pre', 'prefix-mismatch'],
    ['live-late-pre', 'post-live-value-deadline'], ['snap-late', 'post-observation-deadline'],
    ['late-ack-value', 'post-observation-deadline'], ['transform-second', 'unexpected-prefix'],
  ]);
  for (const [mode, reason] of expected) {
    const actual = row(mode), record = diagnostic(actual);
    assert.equal(record.reason, reason, mode);
    assert(actual.failures > 0, mode); assert.equal(actual.returnedBool, false, mode);
    assert.equal(actual.submits, 0, mode);
    if (mode.startsWith('snap-')) { assert.equal(actual.snapshots, 1, mode); assert.equal(actual.attempted, 0, mode); }
  }
  assert.match(diagnostic(row('snap-wide-frontier')).shape, /^scanned=F,visited=1,queued=0,/);
  assert.equal(diagnostic(row('live-nil-pre')).shape, 'UNAVAILABLE');
});

test('actual Create emits no successful-state metadata and no duplicate refusal cascades', () => {
  for (const mode of ['plain', 'coordinate', 'old-text', 'placeholder', 'unicode', 'value', 'word', 'empty',
    'reflow', 'literal-placeholder', 'native-ax-cost', 'pending-render']) assert.deepEqual(row(mode).diagnostics, [], mode);
  for (const actual of rows) {
    assert(actual.diagnostics.length <= 1, actual.mode);
    for (const line of actual.diagnostics) {
      assert(Buffer.byteLength(line) <= 512, actual.mode);
      assert(!/CANARY|fixture-alpha|secondary|Placeholder/.test(line), actual.mode);
    }
  }
});

// Explicit reclassification: ignored snapshot value is not authoritative text.
test('actual Create snapshot-only value metadata permits exact fresh live String', () => {
  originalSuccess('plain'); acknowledged('plain');
  const baseline = row('snap-nil-value');
  assert(baseline.snapshotValueClasses.includes('nil'), 'Actual copied snapshot carries nil metadata');
  assert.equal(baseline.submits, 0);
  if (baseline.returnedBool === false) {
    assert(baseline.failures > 0); assert.equal(baseline.attempted, 0);
    assert.equal(baseline.scalarReadsBySnapshot.length, 0, 'Original snapshot gate stopped before scalar');
    assert.equal(baseline.diagnostics.length, 1);
    assert.match(baseline.diagnostics[0], /^E2E: CREATE_INPUT_REFUSAL phase=PRE_SEND reason=field-value-nil shape=scanned=T,.*modal=ONE,viewport=ONE,field=ONE$/);
  }
  assert.equal(baseline.returnedBool, true, 'Exact fresh live String must survive irrelevant snapshot nil');
  for (const mode of ['snap-nil-value', 'snap-nonstring', 'snap-canary-other', 'snap-canary-string',
    'snap-nil-placeholder', 'snap-nil-old', 'snap-nonstring-placeholder', 'snap-nonstring-old']) {
    const actual = acknowledged(mode); assert.deepEqual(actual.diagnostics, [], mode);
    assert.equal(actual.snapshotValueReads, 0, 'Snapshot text is ignored, never coerced to empty');
  }
  assert.equal(row('snap-nil-old').deletions, 4); assert.equal(row('snap-nonstring-old').deletions, 4);
  assert.equal(row('snap-nil-placeholder').deletions, 0); assert.equal(row('snap-nonstring-placeholder').deletions, 0);
});

test('actual Create uses one fresh scalar after each valid scope and no scalar after rejected scope', () => {
  for (const mode of ['plain', 'pending-render', 'old-text', 'literal-placeholder', 'snap-nil-value', 'snap-nonstring']) {
    const actual = acknowledged(mode);
    assert.equal(actual.scalarReadsBySnapshot.length, actual.snapshots, mode);
    assert.equal(actual.snapshotValueReads, 0, mode);
    for (const { snapshot, reads } of actual.scalarReadsBySnapshot) {
      assert.equal(reads, 1, mode);
      const scope = actual.events.indexOf('scope:' + snapshot), scalar = actual.events.indexOf('scalar:' + snapshot);
      assert(scope >= 0 && scalar > scope, 'Fresh scope precedes one actual scalar attempt');
    }
  }
  for (const mode of ['snap-foreign-field', 'snap-foreign-viewport', 'snap-duplicate-field', 'snap-wrong-type',
    'snap-missing-field', 'snap-offclip', 'snap-wide-frontier', 'snap-late']) {
    assert.deepEqual(row(mode).scalarReadsBySnapshot, [], mode);
    assert.equal(row(mode).attempted, 0, mode); assert.equal(row(mode).submits, 0, mode);
  }
});

test('actual Create live nil other wrong and late scalar refuse before NEXT without healing', () => {
  for (const [mode, count, reason, returned] of [
    ['live-nil-pre', 0, 'live-value-nil', 'nil'], ['live-other-pre', 0, 'live-value-other', 'other'],
    ['live-wrong-pre', 0, 'prefix-mismatch', 'string'], ['live-late-pre', 0, 'post-live-value-deadline', 'string'],
    ['live-nil-ack', 3, 'live-value-nil', 'nil'], ['live-other-ack', 3, 'live-value-other', 'other'],
    ['live-wrong-ack', 3, 'unexpected-prefix', 'string'], ['live-late-ack', 3, 'post-live-value-deadline', 'string'],
  ]) {
    const actual = row(mode); assert(actual.failures > 0, mode);
    assert.equal(actual.returnedBool, false, mode); assert.equal(actual.attempted, count, mode);
    assert.equal(actual.fieldCalls, count === 0 ? 0 : 1, mode); assert.equal(actual.applicationCalls, 0, mode);
    assert.equal(actual.submits, 0, mode); assert.equal(actual.snapshotValueReads, 0, mode);
    assert.equal(actual.liveReturnClasses.at(-1), returned, 'Record returned class after fault, never backing String');
    assert.equal(diagnostic(actual).reason, reason, mode);
    for (const observation of actual.scalarReadsBySnapshot) assert.equal(observation.reads, 1, mode);
    if (returned !== 'string') assert(actual.events.includes('read:' + count + ':' + returned), mode);
    if (mode.includes('wrong')) assert(actual.events.includes('read:' + count + ':other'), mode);
    if (mode.includes('late')) assert(actual.elapsed <= 61, 'One approved60s budget; no renewal');
  }
  assert.equal(row('drop-first-five').attempted, 3); assert.equal(row('drop-first-five').returnedBool, false);
});

test('actual Create deferred first waiter poll must not consume ready ACK budget', () => {
  originalSuccess('plain'); acknowledged('plain');
  const actual = row('deferred-first-poll');
  assert.equal(actual.requested, 13); assert(actual.firstPollCharges > 0, 'Same closed delay port actually ran');
  assert.equal(actual.inputPrefixExact, true, 'No loss or corruption injected');
  assert.equal(actual.applicationCalls, 0); assert.equal(actual.submits, 0);
  assert.equal(actual.fieldCalls, Math.ceil(actual.attempted / 3), 'Logical stream once; max3 physical calls');
  if (actual.returnedBool === false) {
    assert(actual.failures > 0); assert(actual.attempted < actual.requested);
    assert(actual.ackWaiterBudgets.length > 0, 'Original body paid deferred ACK wait cost');
    assert(actual.elapsed <= 61, 'Approved shared60s budget was not renewed');
  }
  assert.equal(actual.finalExact, true, 'Already-ready exact ACK must fit shared phase despite deferred waiter port');
  acknowledged('deferred-first-poll');
  assert.equal(actual.ackWaiterBudgets.length, 0, 'Ready fresh ACK does not enter deferred waiter');
  assert(actual.elapsed <= 30);
});

test('actual Create observes same ready ACK directly and waits only for unready rendering', () => {
  const ready = acknowledged('plain');
  assert.equal(ready.ackWaiterBudgets.length, 0);
  assert.equal(ready.scalarReadsBySnapshot.length, ready.snapshots);
  const pending = acknowledged('pending-render');
  assert(pending.ackWaiterBudgets.length > 0, 'Unready original predicate still uses existing waiter');
  assert(pending.ackWaiterBudgets.every(budget => budget > 0 && budget <= 60));
  assert.equal(pending.fieldCalls, Math.ceil(pending.requested / 3) + pending.deletions, 'Waiting never resubmits a chunk');
  for (const mode of ['live-nil-ack', 'live-other-ack', 'live-wrong-ack', 'live-late-ack']) {
    const actual = row(mode); assert(actual.failures > 0); assert.equal(actual.returnedBool, false);
    assert.equal(actual.attempted, 3); assert.equal(actual.fieldCalls, 1); assert.equal(actual.submits, 0);
    assert.equal(actual.ackWaiterBudgets.length, 0, 'Known invalid/expired first observation cannot be deferred');
  }
});

test('actual Create max3 physical call census follows complete original logical transcript', () => {
  const name = originalSuccess('plain'), value = originalSuccess('value-nineteen');
  assert.equal(name.requested, 13); assert.equal(value.requested, 19);
  assert.equal(name.applicationCalls + value.applicationCalls, 0);
  assert.equal(name.fieldCalls, 5, '13 original Characters require five bounded native calls');
  assert.equal(value.fieldCalls, 7, '19 original Characters require seven bounded native calls');
  assert.equal(3 * (name.fieldCalls + value.fieldCalls), 36, 'Three-secret arithmetic; not native timing');
});
test('actual Create chunk boundaries preserve Unicode and final one or two Characters', () => {
  for (const [mode, sizes] of [['plain', [3,3,3,3,1]], ['value-nineteen', [3,3,3,3,3,3,1]],
    ['unicode', [3]], ['unicode-four', [3,1]], ['unicode-five', [3,2]], ['empty', []]]) {
    const actual = acknowledged(mode); assert.deepEqual(actual.inputCallSizes, sizes, mode);
    assert(actual.deleteCallSizes.every(size => size === 0), mode);
  }
});
test('actual Create missing interior reordered or later chunk refuses without retry or NEXT', () => {
  for (const [mode, attempts, calls] of [['drop-first-one',3,1], ['drop-second',3,1], ['drop-third',3,1],
    ['reorder-first-chunk',3,1], ['duplicate-second',3,1], ['drop-fifth',6,2]]) {
    const actual = row(mode); assert(actual.failures > 0, mode);
    assert.equal(actual.attempted, attempts, mode); assert.equal(actual.fieldCalls, calls, mode);
    assert.equal(actual.inputPrefixExact, true, mode); assert.equal(actual.returnedBool, false, mode);
    assert.equal(actual.applicationCalls, 0, mode); assert.equal(actual.submits, 0, mode);
  }
});

// Owner-approved timing delta only; original non-time assertions and tighter happy-cost controls remain.
test('approved single60 field budget permits exact31s observations without renewing input', () => {
  for (const mode of ['within60-hittable','within60-ack','within60-final','cumulative-within60']) {
    const actual = acknowledged(mode);
    assert(actual.elapsed > 30 && actual.elapsed < 60, mode);
    assert(actual.reachBudgets.length >= 2, mode);
    assert(actual.reachBudgets.every(budget => budget > 0 && budget <= 30), 'Original reachability API maximum30');
  }
});
test('approved shared60 remains strict at FINAL60 and FINAL61 without reusing last ACK', () => {
  for (const mode of ['exact60-final','past60-final']) {
    const actual=row(mode);assert(actual.failures>0);assert.equal(actual.returnedBool,false);
    assert.equal(actual.attempted,actual.requested);assert.equal(actual.finalExact,true,'Typed backing value alone cannot grant FINAL');
    assert.equal(diagnostic(actual).phase,'FINAL');assert.equal(diagnostic(actual).reason,'post-observation-deadline');
    assert.equal(actual.scalarReadsBySnapshot.length,actual.snapshots-1,'FINAL late scope cannot read live value after deadline');
    assert.equal(actual.applicationCalls,0);assert.equal(actual.submits,0);
  }
});
test('approved60 is from field entry, never per chunk or acknowledgement', () => {
  const actual=row('cumulative-over60');assert(actual.failures>0);assert.equal(actual.returnedBool,false);
  assert.equal(actual.attempted,15);assert.equal(actual.fieldCalls,5);assert.equal(actual.elapsed,65);
  assert.equal(actual.submits,0);assert.equal(actual.applicationCalls,0);
  assert(actual.reachBudgets.every(budget=>budget>0&&budget<=30));
});
