import assert from 'node:assert/strict';
import test from 'node:test';
import { chmodSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { iosAppProductHashes, hashIosTree } from './ios-ffi-provenance.mjs';
import { hashArtifactTree } from './artifact-tree.mjs';
import { iosBuildProvenanceIdentity } from './ios-consumer-provenance.mjs';
import { recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'ios-consumer-proof-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const products = join(root, 'Build/Products/Debug-iphonesimulator');
  const put = (path, value = path) => { const file = join(root, path); mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, value); chmodSync(file, 0o644); return file; };
  const prefix = 'Build/Products/Debug-iphonesimulator/';
  const files = {
    app: put(`${prefix}MetaSecret.app/MetaSecret`),
    resource: put(`${prefix}MetaSecret.app/Resources/design.json`),
    framework: put(`${prefix}MetaSecret.app/Frameworks/ComposeApp.framework/ComposeApp`),
    runner: put(`${prefix}iosAppUITests-Runner.app/iosAppUITests-Runner`),
    plugin: put(`${prefix}iosAppUITests-Runner.app/PlugIns/iosAppUITests.xctest/iosAppUITests`),
  };
  const current = () => {
    const identity = iosBuildProvenanceIdentity({ derivedDataPath: root, registrationOnly: true });
    return { version: identity.version, sourceFingerprint: 'same-core-compose-source',
      artifacts: { iosBuild: identity.iosBuild, serverImageId: 'same-server' } };
  };
  const manifest = join(root, 'native-proof.json');
  recordArtifactProvenance(manifest, current(), 'approved build receipt');
  return { root, products, files, put, current, manifest };
}

test('actual consumed app packages retain identity when Xcode writes runtime cache outside them', (t) => {
  const f = fixture(t);
  const before = f.current();
  f.put('Build/Intermediates.noindex/XCBuildData/test-session.db', 'new runtime cache');
  f.put('Build/Products/test-session-status.json', 'finished');
  assert.deepEqual(f.current(), before);
  verifyArtifactProvenance({ path: f.manifest, current: f.current() });
  assert.deepEqual(iosAppProductHashes(f.root), Object.fromEntries(Object.keys(before.artifacts.iosBuild)
    .map((name) => [name, hashIosTree(join(f.products, name))])), 'Existing FFI/link verifier default hashes remain unchanged');
});

for (const target of ['app', 'runner', 'plugin', 'framework', 'resource']) {
  test(`a changed consumed ${target} rejects the stored build proof`, (t) => {
    const f = fixture(t);
    writeFileSync(f.files[target], 'tampered bytes');
    assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /artifact keys=iosBuild/);
  });
}

test('package file modes and resource paths remain part of strict consumer identity', (t) => {
  const f = fixture(t);
  chmodSync(f.files.framework, 0o755);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  chmodSync(f.files.framework, 0o644);
  renameSync(f.files.resource, `${f.files.resource}.renamed`);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
});

test('all app bundles are consumed and renamed/missing packages cannot reuse old proof', (t) => {
  const f = fixture(t);
  f.put('Build/Products/Debug-iphonesimulator/Extra.app/Extra');
  assert.equal(Object.keys(f.current().artifacts.iosBuild).length, 3);
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  rmSync(join(f.products, 'Extra.app'), { recursive: true });
  renameSync(join(f.products, 'MetaSecret.app'), join(f.products, 'Renamed.app'));
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /provenance mismatch/);
  rmSync(join(f.products, 'iosAppUITests-Runner.app'), { recursive: true });
  assert.throws(() => f.current(), /app and XCTest runner products missing/);
});

test('old whole-Build manifest version cannot certify the new consumed-package format', (t) => {
  const f = fixture(t);
  const old = f.current(); old.version = 1; old.artifacts.iosBuild = 'old-whole-tree-hash';
  recordArtifactProvenance(f.manifest, old, 'old build format');
  assert.throws(() => verifyArtifactProvenance({ path: f.manifest, current: f.current() }), /version=1->3/);
});

test('legacy whole-Build identity keeps version 1 and the original absent behavior without app prerequisites', (t) => {
  const f = fixture(t);
  rmSync(join(f.root, 'Build'), { recursive: true });
  assert.deepEqual(iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false }),
    { version: 1, iosBuild: hashArtifactTree(join(f.root, 'Build')) });
  assert.throws(() => iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: true }), /build products missing/);
  f.put('Build/Intermediates.noindex/runtime.db', 'before');
  const before = iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false });
  assert.deepEqual(before, { version: 1, iosBuild: hashArtifactTree(join(f.root, 'Build')) });
  f.put('Build/Intermediates.noindex/runtime.db', 'after');
  assert.notDeepEqual(iosBuildProvenanceIdentity({ derivedDataPath: f.root, registrationOnly: false }), before);
});
