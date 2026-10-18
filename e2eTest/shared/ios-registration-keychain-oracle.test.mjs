import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, rmSync, readFileSync, writeFileSync, symlinkSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { validateIosRegistrationKeychainIdentity, readIosRegistrationMasterKeyPresence, resolveIosRegistrationKeychainIdentity, IosRegistrationOraclePrerequisiteError } from './ios-registration-keychain-oracle.mjs';

const namespace = '7RKQD29YMQ.org.metasecret.vault';
const identity = () => validateIosRegistrationKeychainIdentity({
  bundleInfo: { CFBundleIdentifier: 'org.metasecret.vault' },
  entitlements: { 'application-identifier': namespace },
});

test('namespace is restricted to signed exact app ID, including implicit default group', () => {
  assert.equal(identity().accessGroup, namespace);
  assert.throws(() => validateIosRegistrationKeychainIdentity({ bundleInfo: { CFBundleIdentifier: 'org.metasecret.vault.UITests' }, entitlements: {} }));
  assert.throws(() => validateIosRegistrationKeychainIdentity({ bundleInfo: { CFBundleIdentifier: 'org.metasecret.vault' }, entitlements: {} }));
  assert.throws(() => validateIosRegistrationKeychainIdentity({ bundleInfo: { CFBundleIdentifier: 'org.metasecret.vault' },
    entitlements: { 'application-identifier': namespace, 'keychain-access-groups': ['7RKQD29YMQ.shared'] } }));
});

const indexBlob = (value) => `X'${createHash('sha1').update(value, 'utf8').digest('hex')}'`;

function keychainMetadataFixture(schema = 'svce BLOB,acct BLOB,agrp TEXT,tomb INTEGER,sync INTEGER') {
  const root = mkdtempSync(join(tmpdir(), 'test51-keychain-metadata-'));
  const udid = '00000000-0000-4000-8000-000000000051';
  const dir = join(root, udid, 'data/Library/Keychains');
  mkdirSync(dir, { recursive: true });
  const db = join(dir, 'keychain-2-debug.db');
  const sql = (statement) => {
    const result = spawnSync('python3', ['-c', 'import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.executescript(sys.argv[2]); c.close()', db, statement], { encoding: 'utf8' });
    assert.equal(result.status, 0, 'Synthetic metadata fixture setup failed');
  };
  sql(`CREATE TABLE genp(${schema});`);
  const read = () => readIosRegistrationMasterKeyPresence({ simulatorUdid: udid, identity: identity(), devicesRoot: root });
  return { root, udid, sql, read, close: () => rmSync(root, { recursive: true, force: true }) };
}

test('actual Apple-format SHA1 BLOB indexes locate the exact known service/account', () => {
  const fixture = keychainMetadataFixture();
  try {
    fixture.sql(`INSERT INTO genp VALUES(${indexBlob('MetaSecret')},${indexBlob('master_key')},'${namespace}',0,0);`);
    assert.equal(fixture.read(), true);
  } finally { fixture.close(); }
});

test('read-only hashed metadata observes 0 → 1 → 0 and excludes plain/unrelated/tombstone/cloud decoys', () => {
  const fixture = keychainMetadataFixture();
  const service = indexBlob('MetaSecret'); const account = indexBlob('master_key');
  try {
    fixture.sql(`INSERT INTO genp VALUES('MetaSecret','master_key','${namespace}',0,0);
      INSERT INTO genp VALUES(CAST('MetaSecret' AS BLOB),CAST('master_key' AS BLOB),'${namespace}',0,0);
      INSERT INTO genp VALUES(${indexBlob('Other')},${account},'${namespace}',0,0);
      INSERT INTO genp VALUES(${service},${indexBlob('other_account')},'${namespace}',0,0);
      INSERT INTO genp VALUES(${service},${account},'other.app',0,0);
      INSERT INTO genp VALUES(${service},${account},'${namespace}',1,0);
      INSERT INTO genp VALUES(${service},${account},'${namespace}',0,1);`);
    assert.equal(fixture.read(), false);
    fixture.sql(`INSERT INTO genp VALUES(${service},${account},'${namespace}',0,0);`);
    assert.equal(fixture.read(), true);
    fixture.sql(`DELETE FROM genp WHERE agrp='${namespace}' AND svce=${service} AND acct=${account} AND tomb=0 AND sync=0;`);
    assert.equal(fixture.read(), false);
    assert.throws(() => readIosRegistrationMasterKeyPresence({ simulatorUdid: '../other', identity: identity(), devicesRoot: fixture.root }));
    assert.throws(() => readIosRegistrationMasterKeyPresence({ simulatorUdid: fixture.udid, identity: { accessGroup: namespace }, devicesRoot: fixture.root }));
  } finally { fixture.close(); }
});

test('duplicate exact hashed records are ambiguous rather than accepted as presence', () => {
  const fixture = keychainMetadataFixture();
  try {
    fixture.sql(`INSERT INTO genp VALUES(${indexBlob('MetaSecret')},${indexBlob('master_key')},'${namespace}',0,0);
      INSERT INTO genp VALUES(${indexBlob('MetaSecret')},${indexBlob('master_key')},'${namespace}',0,0);`);
    assert.throws(fixture.read, IosRegistrationOraclePrerequisiteError);
  } finally { fixture.close(); }
});

test('missing required keychain metadata schema fails closed', () => {
  const fixture = keychainMetadataFixture('svce BLOB,acct BLOB,agrp TEXT');
  try { assert.throws(fixture.read, IosRegistrationOraclePrerequisiteError); }
  finally { fixture.close(); }
});

test('runtime SQL uses only exact hashed constants and read-only scoped COUNT predicates', () => {
  const source = readFileSync(new URL('./ios-registration-keychain-oracle.mjs', import.meta.url), 'utf8');
  assert(source.includes('mode=ro'));
  assert(source.includes('PRAGMA query_only=ON'));
  assert(source.includes('SELECT COUNT(*) FROM genp WHERE svce=? AND acct=? AND agrp=? AND tomb=0 AND sync=0'));
  assert(!/SELECT\s+(?:\*|data|v_Data|acct|agrp)/i.test(source));
  assert(source.includes("(hashlib.sha1(b'MetaSecret').digest(), hashlib.sha1(b'master_key').digest(), sys.argv[2])"));
  const query = source.match(/SELECT COUNT\(\*\) FROM genp[^']+/)[0];
  assert(!query.includes('CAST(') && !query.includes(' OR '));
});

function plist(value) {
  const encode = (item) => typeof item === 'string' ? `<string>${item.replaceAll('&', '&amp;').replaceAll('<', '&lt;')}</string>`
    : Array.isArray(item) ? `<array>${item.map(encode).join('')}</array>`
      : typeof item === 'boolean' ? `<${item ? 'true' : 'false'}/>`
        : `<dict>${Object.entries(item).map(([key, entry]) => `<key>${key}</key>${encode(entry)}`).join('')}</dict>`;
  return `<?xml version="1.0"?><plist version="1.0">${encode(value)}</plist>`;
}

function simulatorMachO(payload, { platform = 7, sections = 1, duplicatePlatform = false, magic = 0xfeedfacf,
  fileType = 2, segment = '__TEXT', sectionSegment = '__TEXT', offsetShift = 0, sizeOverride, segmentSizeOverride, badCommand = false, truncated = false } = {}) {
  const segmentSize = 72 + 80 * sections;
  const commandSize = segmentSize + 24 * (duplicatePlatform ? 2 : 1);
  const sectionOffset = 32 + commandSize + 16;
  const bytes = Buffer.from(payload);
  const file = Buffer.alloc(sectionOffset + bytes.length);
  file.writeUInt32LE(magic, 0); file.writeUInt32LE(0x100000c, 4); file.writeUInt32LE(fileType, 12);
  file.writeUInt32LE(duplicatePlatform ? 3 : 2, 16); file.writeUInt32LE(commandSize, 20);
  file.writeUInt32LE(0x19, 32); file.writeUInt32LE(badCommand ? 4 : segmentSize, 36); file.write(segment, 40);
  file.writeBigUInt64LE(BigInt(segmentSizeOverride ?? file.length), 32 + 48); file.writeUInt32LE(sections, 32 + 64);
  for (let index = 0; index < sections; index += 1) {
    const section = 32 + 72 + index * 80;
    file.write('__entitlements', section); file.write(sectionSegment, section + 16);
    file.writeBigUInt64LE(BigInt(sizeOverride ?? bytes.length), section + 40);
    file.writeUInt32LE(sectionOffset + offsetShift, section + 48);
  }
  for (let index = 0; index < (duplicatePlatform ? 2 : 1); index += 1) {
    const at = 32 + segmentSize + index * 24;
    file.writeUInt32LE(0x32, at); file.writeUInt32LE(24, at + 4); file.writeUInt32LE(platform, at + 8);
  }
  bytes.copy(file, sectionOffset);
  return truncated ? file.subarray(0, file.length - 3) : file;
}

function metadataFixture({ signed = {}, embedded = { 'application-identifier': namespace }, macho = {},
  info = {}, executable = 'MetaSecret', escape = false, signedFailure = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'test51-simulator-entitlements-'));
  const appPath = join(root, 'MetaSecret.app'); mkdirSync(appPath);
  const app = realpathSync(appPath);
  const bundleInfo = { CFBundleIdentifier: 'org.metasecret.vault', CFBundleExecutable: executable,
    CFBundleSupportedPlatforms: ['iPhoneSimulator'], ...info };
  const embeddedXml = typeof embedded === 'string' ? embedded : plist(embedded);
  const signedXml = typeof signed === 'string' ? signed : plist(signed);
  writeFileSync(join(app, 'Info.plist'), JSON.stringify(bundleInfo));
  if (escape) {
    writeFileSync(join(root, 'outside'), simulatorMachO(embeddedXml, macho));
    symlinkSync(join(root, 'outside'), join(app, 'MetaSecret'));
  } else writeFileSync(join(app, 'MetaSecret'), simulatorMachO(embeddedXml, macho));
  const run = (command, args, options) => {
    if (command === '/usr/bin/codesign') return { status: signedFailure ? 1 : 0, stdout: signedXml };
    assert.equal(command, '/usr/bin/plutil');
    if (args.at(-1) === join(app, 'Info.plist')) return { status: 0, stdout: JSON.stringify(bundleInfo) };
    assert.equal(args.at(-1), '-');
    const input = options.input.toString();
    if (input === signedXml && typeof signed === 'object') return { status: 0, stdout: JSON.stringify(signed) };
    if (input === embeddedXml && typeof embedded === 'object') return { status: 0, stdout: JSON.stringify(embedded) };
    return { status: 1, stdout: '' };
  };
  return { app, run, close: () => rmSync(root, { recursive: true, force: true }) };
}

function resolveFixture(options) {
  const fixture = metadataFixture(options);
  try { return resolveIosRegistrationKeychainIdentity({ appBundlePath: fixture.app, run: fixture.run }); }
  finally { fixture.close(); }
}

test('valid signed identity remains authoritative without requiring Simulator executable metadata', () => {
  assert.equal(resolveFixture({ signed: { 'application-identifier': namespace }, macho: { magic: 0 },
    info: { CFBundleSupportedPlatforms: ['iPhoneOS'], CFBundleExecutable: '../not-read' } }).accessGroup, namespace);
});

test('absent or valid-empty adhoc signature uses the unique actual Simulator executable plist', () => {
  for (const signed of ['', {}]) assert.equal(resolveFixture({ signed }).accessGroup, namespace);
});

test('malformed, foreign or nonempty incomplete signed metadata never falls back to valid embedded identity', () => {
  for (const signed of ['malformed', { 'application-identifier': '7RKQD29YMQ.other.app' },
    { 'application-identifier': namespace, 'keychain-access-groups': ['7RKQD29YMQ.shared'] }, { 'get-task-allow': true }]) {
    assert.throws(() => resolveFixture({ signed }), IosRegistrationOraclePrerequisiteError);
  }
  assert.throws(() => resolveFixture({ signedFailure: true }), IosRegistrationOraclePrerequisiteError);
});

test('fallback rejects nonSimulator, FAT, duplicate or malformed Mach-O metadata', () => {
  for (const macho of [{ platform: 2 }, { magic: 0xcafebabe }, { sections: 0 }, { sections: 2 },
    { duplicatePlatform: true }, { fileType: 6 }, { segment: '__DATA' }, { sectionSegment: '__DATA' },
    { offsetShift: 1 }, { sizeOverride: 262145 }, { segmentSizeOverride: 999999 }, { badCommand: true }, { truncated: true }]) {
    assert.throws(() => resolveFixture({ macho }), IosRegistrationOraclePrerequisiteError);
  }
  assert.throws(() => resolveFixture({ info: { CFBundleSupportedPlatforms: ['iPhoneOS'] } }), IosRegistrationOraclePrerequisiteError);
});

test('fallback rejects wrong app, unsafe executable basename and symlink escape', () => {
  for (const info of [{ CFBundleIdentifier: 'org.metasecret.vault.UITests' }, { CFBundleExecutable: '../outside' },
    { CFBundleExecutable: '/tmp/outside' }, { CFBundleExecutable: '' }, { CFBundleExecutable: 'missing' }]) {
    assert.throws(() => resolveFixture({ info }), IosRegistrationOraclePrerequisiteError);
  }
  assert.throws(() => resolveFixture({ escape: true }), IosRegistrationOraclePrerequisiteError);
});

test('embedded invalid plist, app identity or custom keychain group fails the same strict namespace guard', () => {
  for (const embedded of ['not-a-plist', {}, { 'application-identifier': '7RKQD29YMQ.other.app' },
    { 'application-identifier': namespace, 'keychain-access-groups': ['7RKQD29YMQ.shared'] }]) {
    assert.throws(() => resolveFixture({ embedded }), IosRegistrationOraclePrerequisiteError);
  }
});
