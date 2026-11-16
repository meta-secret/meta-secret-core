import assert from 'node:assert/strict';
import test from 'node:test';
import { assertWebGeneratedQr, captureWebIncompleteIdentity, readWebCredentialMetadata, assertWebLocalIdentityCleared } from './web.mjs';

const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aQ3cAAAAASUVORK5CYII=', 'base64');

function qrPage({ decoded = 'fixture@example.com', captureError } = {}) {
  const calls = [];
  const page = {
    getByRole(role, { name }) {
      if (role === 'link' && name === 'Devices') return { click: async () => calls.push('devices') };
      assert.equal(role, 'img'); assert.equal(name, 'Vault name QR code');
      return {
        locator(selector) {
          assert.equal(selector, 'svg');
          return { waitFor: async ({ state }) => { assert.equal(state, 'visible'); calls.push('svg-visible'); } };
        },
        async screenshot(options) {
          assert.deepEqual(options, { type: 'png' });
          calls.push('rendered-png');
          if (captureError) throw captureError;
          return png;
        },
        evaluate() { assert.fail('Decode must use the PNG screenshot, not serialized responsive SVG'); },
      };
    },
    getByTestId(id) {
      if (id === 'add-device') return { click: async () => calls.push('add-device') };
      assert.equal(id, 'vault-qr-name');
      return { innerText: async () => 'fixture@example.com' };
    },
    async evaluate(decoder, imageUrl) {
      calls.push('decode');
      assert.equal(imageUrl, `data:image/png;base64,${png.toString('base64')}`);
      assert.deepEqual(Buffer.from(imageUrl.split(',')[1], 'base64'), png);
      assert.equal(imageUrl.includes('svg'), false);
      assert(decoder.toString().includes('QrScanner.scanImage(pngDataUrl'));
      return decoded;
    },
  };
  return { page, calls };
}

test('real generated QR assertion waits for rendering then decodes the exact locator PNG pixels', async () => {
  const { page, calls } = qrPage();
  await assertWebGeneratedQr(page, 'fixture@example.com');
  assert.deepEqual(calls, ['devices', 'add-device', 'svg-visible', 'rendered-png', 'decode']);
});

test('a different decoded name still fails the exact canonical payload postcondition', async () => {
  await assert.rejects(assertWebGeneratedQr(qrPage({ decoded: 'another@example.com' }).page, 'fixture@example.com'),
    /Actual UI QR must decode to the exact canonical name/);
});

test('failed rendered-pixel capture cannot silently decode an SVG or count as a QR pass', async () => {
  const { page, calls } = qrPage({ captureError: new Error('Rendered pixels unavailable') });
  await assert.rejects(assertWebGeneratedQr(page, 'fixture@example.com'), /Rendered pixels unavailable/);
  assert.equal(calls.includes('decode'), false);
});


const metadataPage = (proof) => ({ evaluate: async (callback) => callback === readWebCredentialMetadata ? proof.databases : { snapshot: proof.snapshot, deviceId: proof.deviceId } });

const rejected = { name: 'family', deviceId: 'old-device' };
const bootstrapProof = () => ({
  snapshot: { kind: 'local', name: '' }, deviceId: 'new-device',
  storedIdentityPresent: true,
  databases: {
    'meta-secret': { exists: true, device: 1, user: 0 },
    'meta-secret-server': { exists: false, device: 0, user: 0 },
    'meta-secret-v-device': { exists: false, device: 0, user: 0 },
    'meta-secret-v-device-2': { exists: false, device: 0, user: 0 },
  },
});

test('Incomplete cleanup permits exactly the new Local bootstrap DeviceCreds', async () => {
  await assertWebLocalIdentityCleared(metadataPage(bootstrapProof()), rejected);
});

test('Incomplete cleanup rejects the same old public device identity after Local', async () => {
  const proof = bootstrapProof(); proof.deviceId = rejected.deviceId;
  await assert.rejects(assertWebLocalIdentityCleared(metadataPage(proof), rejected));
});

test('Incomplete cleanup rejects retained UserCreds in any known database', async () => {
  for (const name of Object.keys(bootstrapProof().databases)) {
    const proof = bootstrapProof(); proof.databases[name] = { exists: true, device: 0, user: 1 };
    if (name === 'meta-secret') proof.databases[name].device = 1;
    await assert.rejects(assertWebLocalIdentityCleared(metadataPage(proof), rejected));
  }
});

test('Incomplete cleanup rejects extra old Device records or missing bootstrap', async () => {
  for (const count of [0, 2]) {
    const proof = bootstrapProof(); proof.databases['meta-secret'].device = count;
    await assert.rejects(assertWebLocalIdentityCleared(metadataPage(proof), rejected));
  }
  const proof = bootstrapProof(); proof.databases['meta-secret-v-device'] = { exists: true, device: 1, user: 0 };
  await assert.rejects(assertWebLocalIdentityCleared(metadataPage(proof), rejected));
});

test('Incomplete cleanup rejects absent or unreadable canonical/schema metadata', async () => {
  for (const changes of [{ snapshot: null }, { snapshot: { kind: 'pending', name: 'family' } }, { deviceId: '' }, { databases: {} }]) {
    await assert.rejects(assertWebLocalIdentityCleared(metadataPage({ ...bootstrapProof(), ...changes }), rejected));
  }
  await assert.rejects(assertWebLocalIdentityCleared({ evaluate: async () => { throw new Error('Identity metadata schema unavailable'); } }, rejected), /schema unavailable/);
});

for (const expectedKind of ['pending', 'declined']) {
  test(`capture requires exact ${expectedKind} Vault and persisted old identity before Back`, async () => {
    const proof = bootstrapProof(); proof.snapshot = { kind: expectedKind, name: 'family' };
    proof.deviceId = 'old-device'; proof.databases['meta-secret'].user = 1;
    assert.deepEqual(await captureWebIncompleteIdentity(metadataPage(proof), 'family', expectedKind), rejected);
    await assert.rejects(captureWebIncompleteIdentity(metadataPage(proof), 'other', expectedKind), /exact canonical incomplete Vault/);
    for (const kind of ['member', 'local', 'available', 'exists', expectedKind === 'pending' ? 'declined' : 'pending']) {
      await assert.rejects(captureWebIncompleteIdentity(metadataPage({ ...proof, snapshot: { kind, name: 'family' } }), 'family', expectedKind));
    }
    for (const deviceId of ['', undefined]) {
      await assert.rejects(captureWebIncompleteIdentity(metadataPage({ ...proof, deviceId }), 'family', expectedKind));
    }
    for (const missing of ['device', 'user']) {
      const without = structuredClone(proof); without.databases['meta-secret'][missing] = 0;
      await assert.rejects(captureWebIncompleteIdentity(metadataPage(without), 'family', expectedKind), /persisted credential records/);
    }
  });
}

test('capture cannot authorize Member or unknown cleanup intent', async () => {
  for (const kind of ['member', undefined, 'local']) {
    await assert.rejects(captureWebIncompleteIdentity(metadataPage(bootstrapProof()), 'family', kind), /Only an incomplete attempt/);
  }
});

async function withFakeIndexedDb(inventory, run) {
  const before = { indexedDB: globalThis.indexedDB, IDBKeyRange: globalThis.IDBKeyRange };
  const calls = [];
  globalThis.IDBKeyRange = { bound: (lower, upper) => ({ lower, upper }) };
  globalThis.indexedDB = {
    databases: async () => Object.keys(inventory).map((name) => ({ name })),
    open(name) {
      calls.push(['open', name]);
      const fixture = inventory[name];
      const request = {};
      request.result = {
        objectStoreNames: { contains: (store) => fixture.schema !== false && store === 'commit_log' },
        transaction(stores, mode) {
          assert.deepEqual(stores, ['commit_log']); assert.equal(mode, 'readonly');
          calls.push(['readonly', name]);
          return { objectStore: () => ({ count(range) {
            assert(range.lower === 'DeviceCreds:index::' || range.lower === 'UserCreds:index::');
            assert.equal(range.upper, `${range.lower}\uffff`);
            calls.push(['count', name, range.lower]);
            const query = { result: range.lower.startsWith('Device') ? fixture.device : fixture.user };
            queueMicrotask(() => fixture.countError ? query.onerror() : query.onsuccess());
            return query;
          } }) };
        },
        close() { calls.push(['close', name]); },
      };
      queueMicrotask(() => request.onsuccess());
      return request;
    },
  };
  try { return await run(calls); }
  finally {
    for (const [name, value] of Object.entries(before)) {
      if (value === undefined) delete globalThis[name]; else globalThis[name] = value;
    }
  }
}

test('real metadata collector counts fixed credential prefixes read-only and excludes unknown databases', async () => {
  await withFakeIndexedDb({ 'meta-secret': { device: 1, user: 0 }, unrelated: { device: 99, user: 99 } }, async (calls) => {
    const metadata = await readWebCredentialMetadata();
    assert.deepEqual(metadata, bootstrapProof().databases);
    assert.deepEqual(calls.map(([action]) => action), ['open', 'readonly', 'count', 'count', 'close']);
    assert(calls.every(([, name]) => name === 'meta-secret'));
  });
});

test('real metadata collector fails unknown schema/count error and always closes opened databases', async () => {
  for (const fixture of [{ schema: false }, { device: 1, user: 0, countError: true }]) {
    await withFakeIndexedDb({ 'meta-secret': fixture }, async (calls) => {
      await assert.rejects(readWebCredentialMetadata(), /Identity metadata (schema|count) unavailable/);
      assert.deepEqual(calls.at(-1), ['close', 'meta-secret']);
    });
  }
});
