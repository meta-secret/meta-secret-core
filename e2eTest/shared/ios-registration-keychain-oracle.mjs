import { spawnSync } from 'node:child_process';
import { existsSync, realpathSync, openSync, readSync, closeSync, fstatSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';

const verifiedIdentity = Symbol('Test51 verified iOS app namespace');
const expectedAppId = 'org.metasecret.vault';
const guid = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i;

export class IosRegistrationOraclePrerequisiteError extends Error {
  constructor(message) { super(message); this.name = 'IosRegistrationOraclePrerequisiteError'; }
}

const fail = (message) => { throw new IosRegistrationOraclePrerequisiteError(message); };

function tool(command, args, input, run = spawnSync) {
  const result = run(command, args, { input, encoding: 'utf8', timeout: 5_000, maxBuffer: 262_144 });
  if (result.status !== 0 || result.error) fail('iOS registration oracle could not verify signed app metadata');
  return result.stdout;
}

/** Pure metadata validation; tests use synthetic metadata, runtime uses codesign/plutil below. */
export function validateIosRegistrationKeychainIdentity({ bundleInfo, entitlements, expectedBundleId = expectedAppId }) {
  if (expectedBundleId !== expectedAppId || bundleInfo.CFBundleIdentifier !== expectedAppId) {
    fail('iOS registration oracle requires the exact MetaSecret app bundle, not its XCTest runner');
  }
  const applicationId = entitlements['application-identifier'];
  if (typeof applicationId !== 'string' || !/^[A-Z0-9]{10}\.org\.metasecret\.vault$/.test(applicationId)) {
    fail('Signed MetaSecret application-identifier is unavailable; cleanup cannot be certified');
  }
  const groups = entitlements['keychain-access-groups'] ?? [];
  if (!Array.isArray(groups) || groups.some((group) => typeof group !== 'string')) {
    fail('Signed MetaSecret keychain group metadata is malformed');
  }
  // Apple: kSecAttrAccessGroup defaults to first keychain group, or app ID if none.
  // https://developer.apple.com/documentation/security/ksecattraccessgroup
  const accessGroup = groups[0] ?? applicationId;
  if (accessGroup !== applicationId) fail('Shared/custom keychain namespace is outside the Test51 oracle scope');
  return Object.freeze({ bundleId: expectedAppId, accessGroup, [verifiedIdentity]: true });
}

function readSimulatorEntitlements(app, bundleInfo) {
  if (bundleInfo.CFBundleIdentifier !== expectedAppId ||
    !Array.isArray(bundleInfo.CFBundleSupportedPlatforms) ||
    bundleInfo.CFBundleSupportedPlatforms.length !== 1 || bundleInfo.CFBundleSupportedPlatforms[0] !== 'iPhoneSimulator') {
    fail('Embedded entitlements require the exact verified Simulator app');
  }
  const name = bundleInfo.CFBundleExecutable;
  if (typeof name !== 'string' || !name || name.length > 255 || name === '.' || name === '..' ||
    name.includes('\\') || name.includes('\0') || basename(name) !== name) fail('Simulator executable path is unsafe');
  const executable = realpathSync(join(app, name));
  if (dirname(executable) !== app) fail('Simulator executable resolved outside its verified app');
  const fd = openSync(executable, 'r');
  try {
    const before = fstatSync(fd);
    const read = (size, position) => {
      if (size <= 0 || !Number.isSafeInteger(size) || position < 0 || position + size > before.size) fail('Simulator Mach-O metadata bounds are invalid');
      const bytes = Buffer.alloc(size);
      let count = 0;
      while (count < size) {
        const received = readSync(fd, bytes, count, size - count, position + count);
        if (!received) fail('Simulator Mach-O metadata is truncated');
        count += received;
      }
      return bytes;
    };
    if (!before.isFile() || !Number.isSafeInteger(before.size)) fail('Simulator executable is not a regular file');
    const header = read(32, 0);
    if (header.readUInt32LE(0) !== 0xfeedfacf || ![0x100000c, 0x1000007].includes(header.readUInt32LE(4)) || header.readUInt32LE(12) !== 2) {
      fail('Simulator fallback requires one thin64 Mach-O executable');
    }
    const commandCount = header.readUInt32LE(16);
    const commandBytes = header.readUInt32LE(20);
    if (!commandCount || commandCount > 4096 || commandBytes > 1048576 || commandBytes < commandCount * 8) fail('Simulator Mach-O load commands are invalid');
    const commands = read(commandBytes, 32);
    const platforms = [];
    const sections = [];
    const field = (offset) => commands.subarray(offset, offset + 16).toString('utf8').split('\0')[0];
    let cursor = 0;
    for (let index = 0; index < commandCount; index += 1) {
      if (cursor + 8 > commands.length) fail('Simulator Mach-O load commands are truncated');
      const kind = commands.readUInt32LE(cursor);
      const size = commands.readUInt32LE(cursor + 4);
      if (size < 8 || size % 8 || cursor + size > commands.length) fail('Simulator Mach-O command size is invalid');
      if (kind === 0x32) {
        if (size < 24 || size !== 24 + 8 * commands.readUInt32LE(cursor + 20)) fail('Simulator platform command is invalid');
        platforms.push(commands.readUInt32LE(cursor + 8));
      }
      if (kind === 0x19) {
        if (size < 72) fail('Simulator segment command is invalid');
        const count = commands.readUInt32LE(cursor + 64);
        if (size !== 72 + count * 80) fail('Simulator section table is invalid');
        for (let section = cursor + 72; section < cursor + size; section += 80) {
          if (field(section) !== '__entitlements') continue;
          if (field(cursor + 8) !== '__TEXT' || field(section + 16) !== '__TEXT') fail('Simulator entitlements section is outside __TEXT');
          const length = commands.readBigUInt64LE(section + 40);
          const offset = commands.readUInt32LE(section + 48);
          const segmentStart = commands.readBigUInt64LE(cursor + 40);
          const segmentEnd = segmentStart + commands.readBigUInt64LE(cursor + 48);
          if (segmentEnd > BigInt(before.size) || !length || length > 262144n || offset < 32 + commandBytes || BigInt(offset) < segmentStart ||
            BigInt(offset) + length > segmentEnd || BigInt(offset) + length > BigInt(before.size) ||
            (commands.readUInt32LE(section + 64) & 0xff) !== 0) fail('Simulator entitlements section bounds are invalid');
          sections.push({ offset, length: Number(length) });
        }
      }
      cursor += size;
    }
    if (cursor !== commands.length || platforms.length !== 1 || platforms[0] !== 7 || sections.length !== 1) {
      fail('Simulator entitlements platform or section is missing or ambiguous');
    }
    const payload = read(sections[0].length, sections[0].offset);
    const after = fstatSync(fd);
    if (before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs) fail('Simulator executable changed during verification');
    return payload;
  } finally { closeSync(fd); }
}

/** Invoke only after the runner has proved the freshly built/installed .app provenance. */
export function resolveIosRegistrationKeychainIdentity({ appBundlePath, expectedBundleId = expectedAppId, run = spawnSync }) {
  const app = realpathSync(appBundlePath);
  if (!app.endsWith('.app')) fail('iOS registration oracle requires the verified app bundle path');
  let bundleInfo;
  let entitlements;
  try {
    bundleInfo = JSON.parse(tool('/usr/bin/plutil', ['-convert', 'json', '-o', '-', join(app, 'Info.plist')], undefined, run));
    const signed = tool('/usr/bin/codesign', ['--display', '--entitlements', ':-', app], undefined, run);
    if (signed.trim() === '') entitlements = {};
    else {
      const begin = signed.indexOf('<?xml');
      const end = signed.indexOf('</plist>');
      if (begin < 0 || end < begin) fail('Signed MetaSecret entitlements are malformed; cleanup cannot be certified');
      entitlements = JSON.parse(tool('/usr/bin/plutil', ['-convert', 'json', '-o', '-', '-'], signed.slice(begin, end + 8), run));
    }
    if (!entitlements || typeof entitlements !== 'object' || Array.isArray(entitlements)) fail('Signed MetaSecret entitlements are malformed');
    // Adhoc Simulator signatures can be empty; use only the verified executable's
    // actual Xcode-linked plist, never project settings or mutable .xcent files.
    if (Object.keys(entitlements).length === 0) {
      const embedded = readSimulatorEntitlements(app, bundleInfo);
      entitlements = JSON.parse(tool('/usr/bin/plutil', ['-convert', 'json', '-o', '-', '-'], embedded, run));
    }
  } catch (error) {
    if (error instanceof IosRegistrationOraclePrerequisiteError) throw error;
    fail('Signed MetaSecret app metadata could not be parsed safely');
  }
  return validateIosRegistrationKeychainIdentity({ bundleInfo, entitlements, expectedBundleId });
}

// Deliberately select COUNT only. Never fetch any key bytes, encrypted values, account lists or unrelated groups.
const countScript = `
import hashlib, pathlib, sqlite3, sys
try:
    p = pathlib.Path(sys.argv[1])
    connection = sqlite3.connect(p.as_uri() + '?mode=ro', timeout=2)
    connection.execute('PRAGMA query_only=ON')
    columns = {row[1] for row in connection.execute('PRAGMA table_info(genp)')}
    if not {'svce','acct','agrp','tomb','sync'}.issubset(columns):
        raise RuntimeError('schema')
    count = connection.execute(
        'SELECT COUNT(*) FROM genp WHERE svce=? AND acct=? AND agrp=? AND tomb=0 AND sync=0',
        (hashlib.sha1(b'MetaSecret').digest(), hashlib.sha1(b'master_key').digest(), sys.argv[2])).fetchone()[0]
    connection.close()
    if count not in (0,1):
        raise RuntimeError('ambiguous')
    print(count)
except Exception:
    print('metadata-oracle-unavailable', file=sys.stderr)
    sys.exit(2)
`;

/** Boolean presence for exactly one account in the selected simulator and verified app group. */
export function readIosRegistrationMasterKeyPresence({ simulatorUdid, identity,
  devicesRoot = join(homedir(), 'Library/Developer/CoreSimulator/Devices'), run = spawnSync }) {
  if (!guid.test(simulatorUdid ?? '')) fail('iOS registration oracle requires one exact simulator UDID');
  if (identity?.[verifiedIdentity] !== true) fail('iOS registration oracle requires a verified signed app namespace');
  const selectedRoot = resolve(devicesRoot, simulatorUdid, 'data/Library/Keychains');
  const files = ['keychain-2-debug.db', 'keychain-2.db'].map((name) => join(selectedRoot, name)).filter(existsSync);
  if (files.length !== 1) fail('Selected simulator Keychain metadata database is missing or ambiguous');
  const file = realpathSync(files[0]);
  if (!file.startsWith(realpathSync(resolve(devicesRoot, simulatorUdid)) + '/')) {
    fail('Selected simulator Keychain metadata resolved outside its owned device');
  }
  const output = tool('python3', ['-c', countScript, file, identity.accessGroup], undefined, run).trim();
  if (output !== '0' && output !== '1') fail('Selected simulator Keychain metadata presence is not unambiguous');
  return output === '1';
}
