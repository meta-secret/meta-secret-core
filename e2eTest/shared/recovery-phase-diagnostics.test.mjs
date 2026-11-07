// Ordinary actual watcher/serializer regression; no SDK/process starts.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import {androidInstrumentationAdbArguments, androidInstrumentationDiagnosticArguments} from './android-instrumentation.mjs';
import {createDiagnosticRedactor} from './diagnostic-redaction.mjs';
const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
function actualBody(text, header) {
  assert.equal(text.split(header).length, 2);
  const start = text.indexOf(header), open = start + header.length - 1;
  assert.equal(text[open], '{');
  let cursor = open + 1, depth = 1;
  while (depth && cursor < text.length) { depth += (text[cursor] === '{') - (text[cursor] === '}'); cursor++; }
  assert.equal(depth, 0); return text.slice(start, cursor);
}

function fixture() {
  const calls = [], token = Object.freeze({SYNTHETIC_PROCESS_PORT: true});
  const ports = {androidInstrumentationAdbArguments, androidInstrumentationDiagnosticArguments,
    redactCommandArguments: createDiagnosticRedactor().redactArguments,
    watchProcessOutput(command, args, options) { calls.push({command, args, options}); return token; }};
  const watch = new Function('ports', `const {${Object.keys(ports).join(',')}}=ports;\n`
    + actualBody(source, 'function watchAndroidInstrumentation(serial, instrumentationArguments, runnerComponent) {')
    + '\nreturn watchAndroidInstrumentation;')(ports);
  return {watch, calls, token};
}

for (const edge of ['canonical', 'quoted-edge']) test('actual watcher hides phase tuple before POSIX diagnostics but preserves real argv: ' + edge, () => {
  const f = fixture(), serial = 'SYNTHETIC_SERIAL', runner = 'synthetic.test/Runner';
  const canonical = JSON.stringify({version: 1, runId: 'SYNTHETIC_RUN', testNumber: 25, attempt: 1,
    blockId: 'secret1-three-device', cycle: 13, logicalStep: 1, mode: 'web-secondary-creator-lifecycle',
    contextId: 'a'.repeat(32), invocationId: '2'.repeat(32), platform: 'android', role: 'receiver'});
  const privateValue = edge === 'canonical' ? canonical : `SYNTHETIC_PRIVATE_EDGE_'_\"_$(unchanged)_\\_${canonical}`;
  const args = ['-e', 'class', 'SyntheticClass#handleRecoveryStep', '-e', 'recoveryPhase', privateValue,
    '-e', 'recoveryPhaseExpected', privateValue, '-e', 'recoveryPhaseSelectedProfile', 'test25:web-secondary-creator-lifecycle',
    '-e', 'recoveryPhaseRequired', 'true', '-e', 'secretValue', 'SYNTHETIC_SECRET_VALUE'];
  const snapshot = structuredClone(args);
  assert.equal(f.watch(serial, args, runner), f.token); assert.equal(f.calls.length, 1);
  assert.deepEqual(args, snapshot, 'Diagnostic transformation cannot mutate real logical arguments');
  assert.deepEqual(f.calls[0].args, androidInstrumentationAdbArguments(serial, snapshot, runner));
  const display = f.calls[0].options.displayArgs.join(' ');
  assert(!display.includes(privateValue) && !display.includes('SYNTHETIC_PRIVATE_EDGE') && !display.includes('a'.repeat(32)));
  assert(!display.includes('SYNTHETIC_SECRET_VALUE'), 'Original generic secret redaction remains');
  assert.equal((display.match(/REDACTED_RECOVERY_PHASE/g) ?? []).length, 2);
  assert(display.includes('SyntheticClass#handleRecoveryStep') && display.includes('test25:web-secondary-creator-lifecycle'));
});

test('actual watcher malformed raw argument refusal stays before any process/display publication', () => {
  for (const args of [['-e', 'recoveryPhase', 'SYNTHETIC\0BAD'], null]) {
    const f = fixture(); assert.throws(() => f.watch('SYNTHETIC_SERIAL', args, 'synthetic.test/Runner'));
    assert.equal(f.calls.length, 0);
  }
  for (const [serial, runner] of [['', 'synthetic.test/Runner'], ['SYNTHETIC_SERIAL', '']]) {
    const f = fixture(); assert.throws(() => f.watch(serial, [], runner), /requires a serial and runner/);
    assert.equal(f.calls.length, 0);
  }
});
