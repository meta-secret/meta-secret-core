import { resolve } from 'node:path';
import { hashArtifactTree } from './artifact-tree.mjs';
import { iosAppProductHashes } from './ios-ffi-provenance.mjs';

export function iosBuildProvenanceIdentity({ derivedDataPath, registrationOnly }) {
  return registrationOnly
    ? { version: 3, iosBuild: iosAppProductHashes(derivedDataPath, { hashTree: hashArtifactTree }) }
    : { version: 1, iosBuild: hashArtifactTree(resolve(derivedDataPath, 'Build')) };
}
