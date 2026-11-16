import { createHash } from 'node:crypto';
import { closeSync, existsSync, fstatSync, lstatSync, openSync, readSync, readdirSync, readlinkSync } from 'node:fs';
import { resolve } from 'node:path';

export function hashArtifactTree(rootPath) {
  const digest = createHash('sha256');
  let buffer;
  if (!existsSync(rootPath)) {
    digest.update('absent');
    return digest.digest('hex');
  }

  const visit = (absolutePath, relativePath) => {
    const stats = lstatSync(absolutePath, { bigint: true });
    const type = stats.isDirectory() ? 'directory' : stats.isSymbolicLink() ? 'symlink' : 'file';
    digest.update(`${relativePath}\0${type}\0${stats.mode & 0o7777n}\0`);
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
    const fd = openSync(absolutePath, 'r');
    let failed = false;
    try {
      const initial = fstatSync(fd, { bigint: true });
      const fields = ['dev', 'ino', 'mode', 'size', 'mtimeNs', 'ctimeNs'];
      if (!initial.isFile() || fields.some(field => initial[field] !== stats[field])) {
        throw new Error('Artifact leaf changed before hashing');
      }
      buffer ??= Buffer.allocUnsafe(1024 * 1024);
      let remaining = initial.size;
      while (remaining > 0n) {
        const length = Number(remaining < BigInt(buffer.length) ? remaining : BigInt(buffer.length));
        const bytesRead = readSync(fd, buffer, 0, length, null);
        if (!Number.isInteger(bytesRead) || bytesRead <= 0 || bytesRead > length) {
          throw new Error('Artifact leaf ended before its initial size');
        }
        digest.update(buffer.subarray(0, bytesRead));
        remaining -= BigInt(bytesRead);
      }
      const final = fstatSync(fd, { bigint: true });
      if (fields.some(field => final[field] !== initial[field])) throw new Error('Artifact leaf changed during hashing');
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      try { closeSync(fd); } catch (error) { if (!failed) throw error; }
    }
  };
  visit(rootPath, '.');
  return digest.digest('hex');
}
