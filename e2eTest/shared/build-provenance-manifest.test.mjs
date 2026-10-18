import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { provenanceManifestPath, recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';

const native = { version: 3, sourceFingerprint: 'native-source', artifacts: {
  native: { archive: 'native-bytes' }, androidFfiSourceFingerprint: 'ffi-source', androidJniLibs: 'jni-bytes',
  wasm: 'wasm-bytes', iosBuild: 'ios-bytes', androidAppApk: 'app-bytes', androidTestApk: 'test-bytes', serverImageId: 'server-image',
} };
const web = { version: 2, scope: ['web'], sourceFingerprint: 'web-source',
  artifacts: { wasm: 'wasm-bytes', serverImageId: 'server-image' } };
const withStore = (run) => {
  const directory = mkdtempSync(join(tmpdir(), 'e2e-provenance-scope-'));
  try { run(directory); } finally { rmSync(directory, { recursive: true, force: true }); }
};

test('actual native → Web → native manifest recording preserves both exact proof records', () => withStore((directory) => {
  const nativePath = provenanceManifestPath(directory, 'native');
  const webPath = provenanceManifestPath(directory, 'web');
  verifyArtifactProvenance({ path: nativePath, current: native, bootstrap: true, evidence: '/approved/native-build-receipt.json' });
  const beforeWeb = readFileSync(nativePath);
  verifyArtifactProvenance({ path: webPath, current: web, bootstrap: true, evidence: '/approved/web-build-receipt.json' });
  assert.notEqual(nativePath, webPath);
  assert.deepEqual(readFileSync(nativePath), beforeWeb, 'Web proof cannot overwrite the earlier native proof');
  const verified = verifyArtifactProvenance({ path: nativePath, current: native });
  assert.equal(verified.version, 3);
  assert.equal(verified.sourceFingerprint, native.sourceFingerprint);
  assert.deepEqual(verified.artifacts, native.artifacts);
  assert.equal(verifyArtifactProvenance({ path: webPath, current: web }).version, 2);
}));

test('source, version and every recorded artifact still reject drift after crossing scopes', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  recordArtifactProvenance(path, native, 'full local build completed');
  for (const key of ['version', 'sourceFingerprint', ...Object.keys(native.artifacts)]) {
    const changed = structuredClone(native);
    if (key === 'version') changed.version = 2;
    else if (key === 'sourceFingerprint') changed.sourceFingerprint = 'changed-source';
    else changed.artifacts[key] = 'changed-artifact';
    assert.throws(() => verifyArtifactProvenance({ path, current: changed }), /Build artifact provenance mismatch/, key);
  }
  assert.throws(() => verifyArtifactProvenance({ path, current: web }), /Build artifact provenance mismatch/);
}));

test('missing proof and bootstrap without the parent evidence still fail closed', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  assert.throws(() => verifyArtifactProvenance({ path, current: native }), /manifest not found/);
  assert.throws(() => verifyArtifactProvenance({ path, current: native, bootstrap: true }), /E2E_BUILD_PROVENANCE_EVIDENCE is required/);
  assert.equal(existsSync(path), false);
}));

test('mismatch diagnostics identify scope/version/source and changed artifact keys without dumping values', () => withStore((directory) => {
  const path = provenanceManifestPath(directory, 'native');
  recordArtifactProvenance(path, native, 'approved build');
  const changed = structuredClone(native);
  changed.artifacts.iosBuild = 'new-ios-build-hash';
  assert.throws(() => verifyArtifactProvenance({ path, current: changed, scope: 'native' }), (error) => {
    assert.match(error.message, /scope=native; version=3->3/);
    assert.match(error.message, /expected source=native-source current source=native-source/);
    assert.match(error.message, /artifact keys=iosBuild$/);
    assert.doesNotMatch(error.message, /new-ios-build-hash|ios-bytes|native-bytes/);
    return true;
  });
}));

test('legacy full runs keep their existing manifest path and scope cannot select arbitrary filenames', () => withStore((directory) => {
  assert.equal(provenanceManifestPath(directory, 'full'), join(directory, 'ui-e2e-build-provenance.json'));
  assert.throws(() => provenanceManifestPath(directory, '../other'), /Unknown build provenance scope/);
  const fullPath = provenanceManifestPath(directory, 'full');
  recordArtifactProvenance(fullPath, native, 'legacy full build');
  assert.deepEqual(verifyArtifactProvenance({ path: fullPath, current: native }).artifacts, native.artifacts);
}));
