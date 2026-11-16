import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { hashArtifactTree } from './artifact-tree.mjs';

const MiB = 1024 ** 2;
const bytes = length => { const value = Buffer.alloc(length); for (let index = 0; index < length; index++) value[index] = index % 251; return value; };
function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(join(tmpdir(), 'artifact-hash-unit-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const put = (name, value = Buffer.alloc(0), mode = 0o640) => {
    const path = join(root, name); fs.mkdirSync(dirname(path), { recursive: true });
    fs.writeFileSync(path, value); fs.chmodSync(path, mode); return path;
  };
  return { root, put };
}
// Independent transcript oracle: explicit fixture order, no production traversal/helper.
function legacy(rows) {
  const digest = createHash('sha256');
  for (const [relative, type, path, payload = ''] of rows) {
    digest.update(relative + '\0' + type + '\0' + (fs.lstatSync(path).mode & 0o7777) + '\0');
    digest.update(payload);
  }
  return digest.digest('hex');
}
function io(paths, behavior, run) {
  const saved = Object.fromEntries(['readFileSync', 'openSync', 'readSync', 'closeSync', 'fstatSync'].map(key => [key, fs[key]]));
  const state = { whole: 0, opened: 0, closed: 0, reads: 0, stats: 0, total: 0, max: 0, buffers: new Set(), options: [] }, owned = new Set();
  fs.readFileSync = (path, ...args) => { if (paths.includes(path)) state.whole++; return saved.readFileSync(path, ...args); };
  fs.openSync = (path, ...args) => {
    if (paths.includes(path) && behavior.openError) throw behavior.openError;
    const fd = saved.openSync(path, ...args); if (paths.includes(path)) { owned.add(fd); state.opened++; } return fd;
  };
  fs.readSync = (fd, buffer, offset, length, position) => {
    if (!owned.has(fd)) return saved.readSync(fd, buffer, offset, length, position);
    state.reads++; state.max = Math.max(state.max, length); state.buffers.add(buffer);
    if (behavior.readError) throw behavior.readError;
    if (behavior.eof) return 0;
    const count = saved.readSync(fd, buffer, offset, Math.min(length, behavior.short ?? length), position);
    state.total += count; if (state.reads === 1) behavior.afterFirstRead?.(saved); return count;
  };
  fs.fstatSync = (fd, options) => {
    const result = saved.fstatSync(fd, options); if (!owned.has(fd)) return result;
    state.stats++; state.options.push(options);
    return behavior.stat ? behavior.stat(result, state.stats) : result;
  };
  fs.closeSync = fd => {
    const tracked = owned.has(fd); const result = saved.closeSync(fd);
    if (tracked) { owned.delete(fd); state.closed++; if (behavior.closeError) throw behavior.closeError; } return result;
  };
  syncBuiltinESMExports();
  try { return run(state); } finally {
    Object.assign(fs, saved); syncBuiltinESMExports();
    for (const fd of owned) saved.closeSync(fd); // A failed test cannot leak its controlled fixture descriptors.
  }
}

test('legacy byte transcript matches absent, empty, binary, nested Unicode/default sort, modes and raw links', t => {
  const f = fixture(t), tree = join(f.root, 'tree'); fs.mkdirSync(tree, { mode: 0o750 });
  const z = f.put('tree/Z.bin', Buffer.from([0, 255, 1, 0, 128])), empty = f.put('tree/a/empty');
  const unicode = f.put('tree/ä', 'fixed unicode'), outside = f.put('outside', 'not part of tree');
  fs.symlinkSync('missing', join(tree, 'dangling')); fs.symlinkSync('../outside', join(tree, 'external-link'));
  fs.symlinkSync('Z.bin', join(tree, '💡'));
  const rows = [['.', 'directory', tree], ['./Z.bin', 'file', z, Buffer.from([0, 255, 1, 0, 128])],
    ['./a', 'directory', join(tree, 'a')], ['./a/empty', 'file', empty], ['./dangling', 'symlink', join(tree, 'dangling'), 'missing'],
    ['./external-link', 'symlink', join(tree, 'external-link'), '../outside'], ['./ä', 'file', unicode, 'fixed unicode'],
    ['./💡', 'symlink', join(tree, '💡'), 'Z.bin']];
  assert.equal(hashArtifactTree(tree), legacy(rows)); const before = hashArtifactTree(tree);
  fs.writeFileSync(outside, 'external target changed'); assert.equal(hashArtifactTree(tree), before, 'Never follow a symlink');
  const emptyDirectory = join(f.root, 'empty'); fs.mkdirSync(emptyDirectory);
  assert.equal(hashArtifactTree(emptyDirectory), legacy([['.', 'directory', emptyDirectory]]));
  assert.equal(hashArtifactTree(empty), legacy([['.', 'file', empty]]));
  for (const path of [join(f.root, 'absent'), join(tree, 'dangling')])
    assert.equal(hashArtifactTree(path), createHash('sha256').update('absent').digest('hex'));
});

test('content, root mode, child mode and raw symlink target remain digest inputs', t => {
  const f = fixture(t), file = f.put('tree/data', 'original'), tree = dirname(file), link = join(tree, 'link');
  fs.chmodSync(tree, 0o750);
  fs.symlinkSync('data', link); let previous = hashArtifactTree(tree);
  for (const change of [() => fs.writeFileSync(file, 'changed'), () => fs.chmodSync(tree, 0o700),
    () => fs.chmodSync(file, 0o600), () => { fs.unlinkSync(link); fs.symlinkSync('../other', link); }]) {
    change(); const current = hashArtifactTree(tree); assert.notEqual(current, previous); previous = current;
  }
});

test('causal bounded reading: exact legacy digest precedes whole-file count1 versus required0', t => {
  const f = fixture(t), data = bytes(2 * MiB + 37), file = f.put('large', data), expected = legacy([['.', 'file', file, data]]);
  io([file], {}, state => {
    assert.equal(hashArtifactTree(file), expected, 'The actual old public function must execute and match its stable byte protocol first');
    assert.equal(state.whole, 0, 'Regular product leaves must never use whole-file readFileSync');
    assert(state.reads > 1); assert(state.max <= MiB); assert.equal(state.buffers.size, 1);
    assert.equal(state.total, data.length); assert.equal(state.opened, 1); assert.equal(state.closed, 1);
    assert(state.options.every(options => options?.bigint === true));
  });
});

test('positive partial reads hash exact slices and reuse one buffer across two leaves', t => {
  const f = fixture(t), firstBytes = bytes(MiB + 11), first = f.put('tree/a', firstBytes), second = f.put('tree/b', 'tail');
  const expected = legacy([['.', 'directory', dirname(first)], ['./a', 'file', first, firstBytes], ['./b', 'file', second, 'tail']]);
  io([first, second], { short: 65531 }, state => {
    assert.equal(hashArtifactTree(dirname(first)), expected); assert(state.reads > 2); assert(state.max <= MiB);
    assert.equal(state.buffers.size, 1); assert.equal(state.total, firstBytes.length + 4); assert.equal(state.closed, 2);
  });
});

test('premature zero/EOF refuses instead of returning a short successful digest and closes its fd', t => {
  const f = fixture(t), file = f.put('large', bytes(MiB + 3));
  io([file], { eof: true }, state => { assert.throws(() => hashArtifactTree(file)); assert.equal(state.opened, 1); assert.equal(state.closed, 1); });
});

test('open/read/close faults propagate and repeated dual faults preserve the first Error without descriptor leaks', t => {
  const f = fixture(t), file = f.put('large', bytes(MiB + 3)), primary = Error('controlled read/open failure'), secondary = Error('controlled close failure');
  io([file], { openError: primary }, state => { assert.throws(() => hashArtifactTree(file), error => error === primary); assert.equal(state.closed, 0); });
  for (const closeError of [undefined, secondary]) io([file], { readError: primary, closeError }, state => {
    for (let index = 0; index < 4; index++) {
      assert.throws(() => hashArtifactTree(file), error => error === primary); assert.equal(state.opened, index + 1); assert.equal(state.closed, index + 1);
    }
  });
  io([file], { closeError: secondary }, state => { assert.throws(() => hashArtifactTree(file), error => error === secondary); assert.equal(state.closed, 1); });
});

for (const kind of ['grow', 'truncate']) test('observed ' + kind + ' refuses, closes and never reads beyond initial fd size', t => {
  const f = fixture(t), data = bytes(2 * MiB + 13), file = f.put('large', data);
  io([file], { afterFirstRead: raw => {
    const fd = raw.openSync(file, kind === 'grow' ? 'a' : 'r+');
    try { if (kind === 'grow') fs.writeSync(fd, 'extra bytes'); else fs.ftruncateSync(fd, 3); } finally { raw.closeSync(fd); }
  } }, state => {
    assert.throws(() => hashArtifactTree(file)); assert(state.total <= data.length); assert.equal(state.closed, 1);
  });
});

for (const field of ['dev', 'ino', 'mode', 'size', 'mtimeNs', 'ctimeNs']) test('final BigInt ' + field + ' drift refuses without a digest', t => {
  const f = fixture(t), file = f.put('data', 'fixed bytes');
  io([file], { stat: (value, count) => count === 1 ? value : Object.assign(Object.create(Object.getPrototypeOf(value)), value,
    { [field]: value[field] + (typeof value[field] === 'bigint' ? 1n : 1) }) }, state => {
    assert.throws(() => hashArtifactTree(file)); assert(state.stats >= 2); assert.equal(state.closed, 1);
  });
});

test('opened fd substitution or prefix-mode mismatch refuses before reading and closes only its fd', t => {
  const f = fixture(t), file = f.put('data', 'fixed bytes');
  for (const field of ['dev', 'ino', 'mode']) io([file], { stat: value => Object.assign(Object.create(Object.getPrototypeOf(value)), value,
    { [field]: value[field] + (typeof value[field] === 'bigint' ? 1n : 1) }) }, state => {
    assert.throws(() => hashArtifactTree(file)); assert.equal(state.reads, 0); assert.equal(state.closed, 1);
  });
});
