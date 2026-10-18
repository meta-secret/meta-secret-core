import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, readlinkSync } from 'node:fs';
import { resolve } from 'node:path';

export function hashArtifactTree(rootPath) {
  const digest = createHash('sha256');
  if (!existsSync(rootPath)) {
    digest.update('absent');
    return digest.digest('hex');
  }

  const visit = (absolutePath, relativePath) => {
    const stats = lstatSync(absolutePath);
    const type = stats.isDirectory() ? 'directory' : stats.isSymbolicLink() ? 'symlink' : 'file';
    digest.update(`${relativePath}\0${type}\0${stats.mode & 0o7777}\0`);
    if (stats.isSymbolicLink()) {
      digest.update(readlinkSync(absolutePath));
      return;
    }
    if (stats.isDirectory()) {
      for (const entry of readdirSync(absolutePath).sort()) {
        visit(resolve(absolutePath, entry), relativePath ? `${relativePath}/${entry}` : entry);
      }
      return;
    }
    digest.update(readFileSync(absolutePath));
  };
  visit(rootPath, '.');
  return digest.digest('hex');
}
