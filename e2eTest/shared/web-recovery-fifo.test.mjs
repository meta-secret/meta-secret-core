import test from 'node:test';
import assert from 'node:assert/strict';
import { waitForFifoCanonicalSnapshot } from './web-recovery-fifo.mjs';

function fixture() {
  const subscribers = new Set(); let reads = 0; const timers = new Map();
  const state = { appManager: {}, currState: { id: 'before' }, sessionGeneration: 1, isSynchronizing: false, requiredSyncPending: false, syncError: '', $subscribe(fn) { subscribers.add(fn); return () => subscribers.delete(fn); } };
  let projection = { session: { ready: true, member: true, vaultId: 'v' }, operations: [], secrets: [], devices: [] };
  const runtime = { state, readOwnedMember: async ({ state: current }, mapper) => { reads++; return mapper(current); }, readCoreProjection: () => structuredClone(projection), setTimeout(fn) { timers.set(1, fn); return 1; }, clearTimeout(id) { timers.delete(id); } };
  return { state, runtime, subscribers, timers, get reads() { return reads; }, publish(next, extra = {}) { projection = next; Object.assign(state, extra); state.currState = { id: 'after' }; for (const callback of subscribers) callback(); }, current: () => structuredClone(projection) };
}
const expected = [{ requesterId: 'sender-a', passId: 'pass-a' }, { requesterId: 'sender-b', passId: 'pass-b' }];
const operation = (requestId, requesterId, secretId, extra = {}) => ({ kind: 'recover', requestId, requesterId, secretId, clientStatus: 'needApprove', eligiblePeerIds: ['receiver'], ...extra });
const pending = [operation('claim-a', 'sender-a', 'pass-a'), operation('claim-b', 'sender-b', 'pass-b')];
const input = { mode: 'pending', expected, baselineRequestIds: ['old-claim'], timeoutMs: 10 };
const nextTurn = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };

test('both canonical pending identities are required before publication; subscription never calls sync/Core command', async () => {
  const f = fixture(); const wait = waitForFifoCanonicalSnapshot(input, f.runtime); await nextTurn();
  f.publish({ ...f.current(), operations: [pending[0]] }); await nextTurn(); assert.equal(f.subscribers.size, 1);
  f.publish({ ...f.current(), operations: pending }); const snapshot = await wait;
  assert.deepEqual(snapshot.operations.map((x) => x.requestId), ['claim-a', 'claim-b']); assert.equal(f.subscribers.size, 0); assert.equal(f.timers.size, 0);
  assert(f.reads >= 2); assert.equal(Object.keys(f.runtime).some((key) => /sync|recover|accept/i.test(key)), false);
});

test('a terminal, foreign or duplicated new claim cannot masquerade as two persisted submissions', async () => {
  for (const operations of [[pending[0], { ...pending[1], outcome: 'accepted' }], [pending[0], operation('foreign', 'other', 'pass-b')], [...pending, operation('duplicate', 'sender-b', 'pass-b')]]) {
    const f = fixture(); const wait = waitForFifoCanonicalSnapshot(input, f.runtime); await nextTurn(); f.publish({ ...f.current(), operations });
    await assert.rejects(wait, /pending|foreign|unique|terminal/i); assert.equal(f.subscribers.size, 0); assert.equal(f.timers.size, 0);
  }
});

test('busy state and stale snapshot cannot publish; current ready publication resolves after the real event', async () => {
  const f = fixture(); f.state.isSynchronizing = true;
  const wait = waitForFifoCanonicalSnapshot(input, f.runtime); await nextTurn(); assert.equal(f.reads, 0);
  f.publish({ ...f.current(), operations: pending }, { isSynchronizing: false }); await wait; assert.equal(f.subscribers.size, 0);
});

test('real sync error, timeout and native session retirement remove the owned subscription', async () => {
  const f = fixture(); const first = waitForFifoCanonicalSnapshot(input, f.runtime); await nextTurn(); f.publish(f.current(), { syncError: 'current synchronization failed' }); await assert.rejects(first, /synchronization/i); assert.equal(f.subscribers.size, 0);
  const g = fixture(); const second = waitForFifoCanonicalSnapshot(input, g.runtime); await nextTurn(); [...g.timers.values()][0](); await assert.rejects(second, /Timed out/); assert.equal(g.subscribers.size, 0);
  const h = fixture(); const third = waitForFifoCanonicalSnapshot(input, h.runtime); await nextTurn(); h.publish(h.current(), { sessionGeneration: 2 }); await assert.rejects(third, /session/i); assert.equal(h.subscribers.size, 0);
});

test('terminal observation requires exact accepted UUID and leaves the unrelated pending claim intact', async () => {
  const f = fixture(); const terminalInput = { mode: 'terminal', requestId: 'claim-b', outcome: 'accepted', pendingRequests: [{ requestId: 'claim-a', requesterId: 'sender-a', passId: 'pass-a' }], expectedRequest: { requesterId: 'sender-b', passId: 'pass-b' }, timeoutMs: 10 };
  const wait = waitForFifoCanonicalSnapshot(terminalInput, f.runtime); await nextTurn();
  f.publish({ ...f.current(), operations: [pending[0], { ...pending[1], outcome: 'accepted', resolutionSource: 'verified', clientStatus: 'accepted' }] });
  const result = await wait; assert.equal(result.operations[1].requestId, 'claim-b'); assert.equal(f.subscribers.size, 0);
});
