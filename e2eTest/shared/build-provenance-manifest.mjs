import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const manifestNames = {
  full: 'ui-e2e-build-provenance.json',
  native: 'ui-e2e-build-provenance-native.json',
  web: 'ui-e2e-build-provenance-web.json',
};

export function provenanceManifestPath(directory, scope) {
  if (!Object.hasOwn(manifestNames, scope)) throw new Error(`Unknown build provenance scope: ${scope}`);
  return resolve(directory, manifestNames[scope]);
}

export function provenanceManifestPaths(directory) {
  return Object.keys(manifestNames).map((scope) => provenanceManifestPath(directory, scope));
}

export function recordArtifactProvenance(path, current, evidence) {
  writeFileSync(path, `${JSON.stringify({ ...current, capturedAt: new Date().toISOString(), evidence }, null, 2)}\n`);
}

export function verifyArtifactProvenance({ path, current, bootstrap = false, evidence, scope = current.scope?.join(',') ?? 'native/full' }) {
  if (bootstrap) {
    if (!evidence) throw new Error('E2E_BUILD_PROVENANCE_EVIDENCE is required to bootstrap artifact provenance');
    recordArtifactProvenance(path, current, evidence);
  } else if (!existsSync(path)) throw new Error(`Proven build artifact manifest not found: ${path}`);
  const expected = JSON.parse(readFileSync(path, 'utf8'));
  // Legacy manifests remain compatible only when both sides omit the tuple.
  if (expected.version !== current.version || expected.sourceFingerprint !== current.sourceFingerprint
    || ['configuration', 'target'].some((key) => JSON.stringify(expected[key]) !== JSON.stringify(current[key]))
    || JSON.stringify(expected.artifacts) !== JSON.stringify(current.artifacts)) {
    const artifactKeys = [...new Set([...Object.keys(expected.artifacts ?? {}), ...Object.keys(current.artifacts ?? {})])]
      .filter((key) => JSON.stringify(expected.artifacts?.[key]) !== JSON.stringify(current.artifacts?.[key]));
    const artifactsDiffer = JSON.stringify(expected.artifacts) !== JSON.stringify(current.artifacts);
    throw new Error(`Build artifact provenance mismatch: scope=${scope}; version=${expected.version ?? '<missing>'}->${current.version}; `
      + `expected source=${expected.sourceFingerprint ?? '<missing>'} current source=${current.sourceFingerprint}; `
      + `artifact keys=${artifactKeys.join(',') || (artifactsDiffer ? '<order-only>' : '<none>')}`);
  }
  return expected;
}
