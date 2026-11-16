import { test } from 'node:test';
import assert from 'node:assert/strict';
import { installWebOperationObserver, readWebOperationObserver, releaseWebOperationObserver, installWebAuthnAbort, abortWebAuthn } from './web-operation-observer.mjs';

const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const pass = (id = 'opaque', name = 'selected') => ({ __wbg_ptr: 7, name, id_str() { if (!this.__wbg_ptr) throw Error('freed'); return id; } });

test('observer preserves receiver, exact arguments and return identity; no eager or duplicate Core call', async () => {
  const pending = deferred(), value = { receipt: true }, borrowed = pass(), calls = [];
  const original = function (...args) { calls.push({ receiver: this, args }); return pending.promise; };
  const manager = { delete_secret_checked: original }, registry = {};
  await installWebOperationObserver({ manager, registry, scope: 'delete', method: 'delete_secret_checked', expectedId: 'opaque', borrowed: true });
  assert.equal(calls.length, 0);
  const result = manager.delete_secret_checked(borrowed, 'unchanged');
  assert.equal(readWebOperationObserver({ registry, scope: 'delete' }).phase, 'entered');
  assert.deepEqual(calls, [{ receiver: manager, args: [borrowed, 'unchanged'] }]);
  pending.resolve(value);
  assert.equal(await result, value);
  assert.equal(manager.delete_secret_checked, original);
  assert.equal(readWebOperationObserver({ registry, scope: 'delete' }).borrowedAlive, true);
  borrowed.__wbg_ptr = 0; // The legitimate withOwnedPass finally can release after callback return.
  assert.equal(readWebOperationObserver({ registry, scope: 'delete' }).borrowedAlive, true);
  assert.equal(calls.length, 1);
});

test('foreign calls cannot satisfy the scoped observer; original errors are rethrown and restored', async () => {
  const error = Error('private diagnostic'), calls = [];
  const original = function (value) { calls.push(value); if (value.id_str() === 'opaque') throw error; return value; };
  const manager = { show_local_secret: original }, registry = {};
  await installWebOperationObserver({ manager, registry, scope: 'show', method: 'show_local_secret', expectedId: 'opaque' });
  const foreign = pass('foreign'); assert.equal(await manager.show_local_secret(foreign), foreign);
  assert.equal(readWebOperationObserver({ registry, scope: 'show' }).phase, 'armed');
  await assert.rejects(manager.show_local_secret(pass()), (failure) => failure === error);
  assert.equal(manager.show_local_secret, original);
  assert.equal(readWebOperationObserver({ registry, scope: 'show' }).phase, 'rejected');
  assert.equal(JSON.stringify(readWebOperationObserver({ registry, scope: 'show' })).includes('private diagnostic'), false);
  assert.equal(calls.length, 2);
});

test('late Show holds only the actual returned value until release; metadata contains no plaintext', async () => {
  const manager = { show_local_secret: async () => 'sensitive synthetic value' }, registry = {};
  await installWebOperationObserver({ manager, registry, scope: 'late', method: 'show_local_secret', expectedId: 'opaque', holdResult: true });
  let returned = false;
  const result = manager.show_local_secret(pass()).then((value) => { returned = true; return value; });
  await Promise.resolve(); await Promise.resolve();
  const snapshot = readWebOperationObserver({ registry, scope: 'late' });
  assert.equal(snapshot.phase, 'held'); assert.equal(returned, false);
  assert.equal(JSON.stringify(snapshot).includes('sensitive'), false);
  releaseWebOperationObserver({ registry, scope: 'late' });
  assert.equal(await result, 'sensitive synthetic value'); assert.equal(returned, true);
});

test('consumed or substituted borrowed identity fails the ownership oracle without changing operation truth', async () => {
  const borrowed = pass(); const receipt = { phase: 'queued' };
  const manager = { delete_secret_checked: async (value) => { value.__wbg_ptr = 0; return receipt; } }, registry = {};
  await installWebOperationObserver({ manager, registry, scope: 'bad', method: 'delete_secret_checked', expectedId: 'opaque', borrowed: true });
  assert.equal(await manager.delete_secret_checked(borrowed), receipt);
  assert.equal(readWebOperationObserver({ registry, scope: 'bad' }).borrowedAlive, false);
});

test('browser abort forwards the ORIGINAL credential API with exact publicKey and a real signal', async () => {
  const publicKey = { challenge: new Uint8Array([1, 2, 3]), userVerification: 'required' };
  const registry = {}, calls = [];
  const original = function (options) {
    calls.push({ receiver: this, options });
    return new Promise((_, reject) => options.signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true }));
  };
  const credentials = { get: original };
  installWebAuthnAbort({ credentials, registry, scope: 'auth' });
  assert.equal(calls.length, 0);
  assert.throws(() => abortWebAuthn({ registry, scope: 'auth' }), /actual entered/);
  const result = credentials.get({ publicKey, mediation: 'optional' });
  assert.equal(calls.length, 1); assert.equal(calls[0].receiver, credentials);
  assert.equal(calls[0].options.publicKey, publicKey);
  assert.equal(calls[0].options.mediation, 'optional');
  assert(calls[0].options.signal instanceof AbortSignal);
  assert.equal(calls[0].options.signal.aborted, false);
  abortWebAuthn({ registry, scope: 'auth' });
  await assert.rejects(result, (error) => error.name === 'AbortError');
  assert.equal(registry.auth.outcome, 'AbortError'); assert.equal(credentials.get, original);
});
