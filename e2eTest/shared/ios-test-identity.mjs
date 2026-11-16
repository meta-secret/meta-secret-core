import { createHash, randomUUID } from 'node:crypto';
import { closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { EXISTING_IOS_TARGET } from './platform-supervisor.mjs';

export const IOS_NAMESPACE_FLAG = 'METASECRET_E2E_IDENTITY_NAMESPACE';
export const IOS_E2E_CONFIGURATION = Object.freeze({ environment: 'local', configuration: 'Debug', identityRequired: 'YES' });
const forbiddenMode = ['METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE'];
const e2eRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const digest = value => createHash('sha256').update(value).digest('hex');
const check = (condition, message) => { if (!condition) throw Error(message); };
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const validNamespace = value => typeof value === 'string' && /^f2-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value);
const safeId = value => typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const within = (root, path) => path === root || path.startsWith(root + sep);
const keys = value => Object.keys(value).sort().join(',');
const contextKeys = 'attempt,bundleId,runId,runRoot,scenarioKey,testNumber,udid';

export function assertIosIdentityAdmission({ testNumber, scenario }) {
  if (!scenario?.ios) return;
  check(Number.isInteger(testNumber) && testNumber >= 1 && testNumber <= 50 &&
    scenario.mode !== 'registration-only', 'iOS namespace route is ineligible; Test51 remains blocked');
  check(scenario.ios.simulatorName === EXISTING_IOS_TARGET.simulatorName &&
    scenario.ios.bundleId === 'org.metasecret.vault', 'iOS namespace target mismatch');
}
function ownedDirectory(path) {
  const stat = lstatSync(path);
  check(stat.isDirectory() && !stat.isSymbolicLink() && stat.uid === process.getuid() &&
    !(stat.mode & 0o022) && realpathSync(path) === path, 'iOS identity directory ownership/path mismatch');
}
function validateContext(context) {
  check(context && keys(context) === contextKeys && safeId(context.runId) &&
    Number.isSafeInteger(context.attempt) && context.attempt > 0 &&
    Number.isInteger(context.testNumber) && context.testNumber >= 1 && context.testNumber <= 50 &&
    context.udid === EXISTING_IOS_TARGET.udid && context.bundleId === 'org.metasecret.vault',
  'Invalid immutable iOS identity context');
  check(typeof context.runRoot === 'string' && resolve(context.runRoot) === context.runRoot &&
    typeof context.scenarioKey === 'string' && context.scenarioKey.length <= 2048,
  'Invalid iOS identity context path');
  if (context.scenarioKey.startsWith('block:')) {
    check(context.testNumber === 50 && safeId(context.scenarioKey.slice(6)), 'Invalid Test50 identity block');
  } else {
    check(context.scenarioKey.startsWith('file:'), 'Invalid iOS scenario identity boundary');
    const file = context.scenarioKey.slice(5);
    check(realpathSync(file) === file && (within(e2eRoot, file) || within(context.runRoot, file)),
      'Foreign iOS scenario identity boundary');
  }
  ownedDirectory(context.runRoot);
  return context;
}
export function createIosIdentityContext({ runRoot, runId, testNumber, attempt, scenarioPath, scenario, udid }) {
  assertIosIdentityAdmission({ testNumber, scenario });
  check(scenario?.ios && safeId(runId), 'A declared iOS invocation identity is required');
  runRoot = resolve(runRoot);
  if (!existsSync(runRoot)) {
    check(runRoot === resolve(e2eRoot, '.artifacts/runs', runId), 'Missing foreign iOS identity run directory');
    mkdirSync(runRoot, { recursive: true, mode: 0o700 });
  }
  const scenarioKey = scenario.deletionBlock
    ? 'block:' + scenario.deletionBlock.id : 'file:' + realpathSync(scenarioPath);
  return Object.freeze(validateContext({ runRoot, runId, testNumber, attempt, scenarioKey, udid, bundleId: scenario.ios.bundleId }));
}
function recordLocation(context, create) {
  validateContext(context);
  const directory = resolve(context.runRoot, 'ios-identities');
  if (create && !existsSync(directory)) mkdirSync(directory, { mode: 0o700 });
  ownedDirectory(directory);
  return resolve(directory, digest(JSON.stringify(context)) + '.json');
}
function readScope(context, recordPath) {
  try {
    const stat = lstatSync(recordPath);
    check(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1 && stat.uid === process.getuid() &&
      (stat.mode & 0o777) === 0o600 && stat.size > 0 && stat.size <= 4096, 'Invalid scope file');
    const raw = readFileSync(recordPath), record = JSON.parse(raw);
    check(record && keys(record) === 'boundary,namespace,version' && record.version === 1 &&
      same(record.boundary, context) && validNamespace(record.namespace), 'Invalid scope record');
    return Object.freeze({ namespace: record.namespace, recordPath, recordSha256: digest(raw) });
  } catch { throw Error('Isolated iOS identity record is missing, foreign or invalid; no replacement/legacy fallback'); }
}
export function requireIosIdentityBoundary(context) {
  return readScope(context, recordLocation(context, false));
}
export function prepareIosIdentityBoundary(context, { resume = false } = {}) {
  const recordPath = recordLocation(context, !resume);
  if (existsSync(recordPath)) return readScope(context, recordPath);
  check(!resume, 'Resumed iOS identity record is missing; no replacement');
  const record = { version: 1, boundary: context, namespace: 'f2-' + randomUUID() };
  let descriptor;
  try {
    descriptor = openSync(recordPath, 'wx', 0o600);
    writeFileSync(descriptor, JSON.stringify(record) + '\n');
  } catch (error) {
    if (error.code !== 'EEXIST') throw Error('Isolated iOS identity creation failed');
  } finally { if (descriptor !== undefined) closeSync(descriptor); }
  return readScope(context, recordPath);
}
export function assertRealOsModeAbsent(...values) {
  const pending = [...values]; let visited = 0;
  while (pending.length) {
    check(++visited <= 8192, 'OS environment inspection exceeded its bound');
    const value = pending.pop();
    if (!value || typeof value !== 'object') continue;
    check(!forbiddenMode.some(key => Object.hasOwn(value, key)), 'Real OS namespace transport contains UI_TEST_MODE');
    pending.push(...Object.values(value).filter(item => item && typeof item === 'object'));
  }
}
export function requireIosIdentityBuild(appPath, {
  readMarker = path => execFileSync('/usr/libexec/PlistBuddy', ['-c', 'Print :MetaSecretE2EIdentityRequired', resolve(path, 'Info.plist')],
    { encoding: 'utf8', timeout: 10000, maxBuffer: 65536 }).trim(),
  verifySignature = path => execFileSync('/usr/bin/codesign', ['--verify', '--strict', '--deep', path],
    { timeout: 15000, maxBuffer: 65536 }),
} = {}) {
  check(readMarker(appPath) === 'YES', 'Required isolated iOS build marker is missing or not YES');
  verifySignature(appPath); return 'YES';
}

export const iosIdentityPlistCopyScript = `import sys,os,json,plistlib,hashlib,copy
source,destination=sys.argv[1:]
payload=json.loads(os.environ['E2E_SECRET_CONFIG'])
assert set(payload)=={'namespace','host','realOs'}
assert isinstance(payload['host'],dict) and all(isinstance(k,str) and isinstance(v,str) for k,v in payload['host'].items())
flag='METASECRET_E2E_IDENTITY_NAMESPACE'
forbidden={'METASECRET_UI_TEST_MODE','SIMCTL_CHILD_METASECRET_UI_TEST_MODE'}
raw=open(source,'rb').read();assert len(raw)<=8*1024*1024
original=plistlib.loads(raw);changed=copy.deepcopy(original)
testroot=os.path.dirname(os.path.abspath(source));relocations=[];pending=[(changed,())];visited=0
while pending:
    visited+=1;assert visited<=8192
    value,path=pending.pop()
    if isinstance(value,dict):pending.extend((v,path+(k,)) for k,v in value.items())
    elif isinstance(value,list):pending.extend((v,path+(i,)) for i,v in enumerate(value))
    elif isinstance(value,str) and '__TESTROOT__' in value:
        parent=changed
        for key in path[:-1]:parent=parent[key]
        replacement=value.replace('__TESTROOT__',testroot)
        parent[path[-1]]=replacement;relocations.append((path,value,replacement))
if payload['realOs']:
    stack=[original];visited=0
    while stack:
        visited+=1;assert visited<=8192
        item=stack.pop()
        if isinstance(item,dict):
            assert not forbidden.intersection(item)
            stack.extend(v for v in item.values() if isinstance(v,(dict,list)))
        elif isinstance(item,list):stack.extend(item)
targets=[t for c in changed.get('TestConfigurations',[]) for t in c.get('TestTargets',[]) if t.get('BlueprintName')=='iosAppUITests']
assert len(targets)==1
target=targets[0];fields=('EnvironmentVariables','UITargetAppEnvironmentVariables');had={f:f in target for f in fields}
for field in fields:
    environment=target.setdefault(field,{})
    assert isinstance(environment,dict) and flag not in environment
    assert not any(k.startswith('LIVEBOARD_RENDERER_') for k in environment)
    environment[flag]=payload['namespace']
host=target[fields[0]]
for key,value in payload['host'].items():
    assert key.startswith('E2E_') and key not in host
    host[key]=value
fd=os.open(destination,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'wb') as output:plistlib.dump(changed,output,fmt=plistlib.FMT_BINARY)
actual=plistlib.loads(open(destination,'rb').read())
observed=[t for c in actual['TestConfigurations'] for t in c['TestTargets'] if t.get('BlueprintName')=='iosAppUITests'][0]
for key,value in payload['host'].items():assert observed[fields[0]].pop(key)==value
for field in fields:
    assert observed[field].pop(flag)==payload['namespace']
    if not had[field]:del observed[field]
for path,before,after in relocations:
    parent=actual
    for key in path[:-1]:parent=parent[key]
    assert parent[path[-1]]==after
    parent[path[-1]]=before
assert actual==original and open(source,'rb').read()==raw
assert os.stat(destination).st_mode&0o777==0o600
print(json.dumps({'originalSha256':hashlib.sha256(raw).hexdigest(),'copySha256':hashlib.sha256(open(destination,'rb').read()).hexdigest(),'semanticInverseExact':True,'mode':'0600'}))
`;
const classes = new Set(['CaseOneJoinFromIosUITest', 'CaseTwoIosInitiatorUITest', 'CaseThreeIosJoinUITest',
  'CaseFourIosConcurrentRecoveryUITest', 'CaseEightIosBothOfflineUITest']);
export function prepareScopedIosInvocation({
  identityContext, templateProof, selector, hostEnvironment, authPolicy, outputDirectory,
  ambientEnvironment = process.env, privateConfiguration,
}, { copyPlistSync = (args, options) => execFileSync('python3', args, options) } = {}) {
  const scope = requireIosIdentityBoundary(identityContext);
  const parts = typeof selector === 'string' ? selector.split('/') : [];
  check(parts.length === 3 && parts[0] === 'iosAppUITests' && classes.has(parts[1]) &&
    /^[A-Za-z_][A-Za-z0-9_]*$/.test(parts[2]) && parts[2] !== 'handleRegistrationForTest51', 'Unreviewed iOS namespace selector');
  check(['controlled', 'real-os', 'controlled-business'].includes(authPolicy), 'Unknown iOS namespace auth policy');
  const coreRouting = privateConfiguration?.requireCoreRouting === true;
  check(authPolicy !== 'controlled' ? identityContext.testNumber === 50 && parts[1] === 'CaseEightIosBothOfflineUITest' && coreRouting : !coreRouting,
    'iOS namespace auth policy/class/routing mismatch');
  check(authPolicy === 'controlled-business' ? privateConfiguration?.authProfile === authPolicy
    : authPolicy === 'real-os' ? privateConfiguration?.authProfile === undefined || privateConfiguration.authProfile === authPolicy
      : privateConfiguration?.authProfile === undefined, 'iOS namespace auth profile mismatch');
  check(hostEnvironment && typeof hostEnvironment === 'object' && !Array.isArray(hostEnvironment) &&
    Object.entries(hostEnvironment).every(([key, value]) => /^E2E_[A-Z0-9_]+$/.test(key) && typeof value === 'string'),
  'Invalid iOS host environment schema');
  if (authPolicy !== 'controlled') assertRealOsModeAbsent(process.env, ambientEnvironment, hostEnvironment, privateConfiguration);
  const original = resolve(templateProof.original);
  const stat = lstatSync(original);
  check(stat.isFile() && !stat.isSymbolicLink() && stat.size > 0 && stat.size <= 8 * 1024 * 1024 &&
    digest(readFileSync(original)) === templateProof.sha256, 'Current owning xctestrun template mismatch');
  outputDirectory = resolve(outputDirectory);
  check(within(identityContext.runRoot, outputDirectory), 'Foreign iOS transport output directory');
  if (!existsSync(outputDirectory)) mkdirSync(outputDirectory, { mode: 0o700 });
  ownedDirectory(outputDirectory);
  const xctestrun = resolve(outputDirectory, 'selected-' + randomUUID() + '.xctestrun');
  // Both Test50 profiles require a clean template; controlled MODE is added only by XCTest.
  const payload = JSON.stringify({ namespace: scope.namespace, host: hostEnvironment, realOs: authPolicy !== 'controlled' });
  check(Buffer.byteLength(payload) <= 2 * 1024 * 1024, 'iOS transport payload exceeds bound');
  const result = copyPlistSync(['-I', '-c', iosIdentityPlistCopyScript, original, xctestrun], {
    env: { ...process.env, E2E_SECRET_CONFIG: payload }, timeout: 15000, maxBuffer: 65536, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  });
  check(digest(readFileSync(original)) === templateProof.sha256 &&
    requireIosIdentityBoundary(identityContext).recordSha256 === scope.recordSha256, 'iOS transport input drift');
  const proof = JSON.parse(String(result));
  check(proof.semanticInverseExact === true && proof.originalSha256 === templateProof.sha256, 'iOS transport inverse proof missing');
  return { xctestrun, namespace: scope.namespace, recordPath: scope.recordPath, proof };
}
export function iosGeneratedTemplateProof(derivedDataPath) {
  const manifest = JSON.parse(readFileSync(resolve(derivedDataPath, 'ios-core-link-provenance.json'), 'utf8'));
  const proof = manifest.generatedTemplate;
  check(proof && dirname(proof.original) === resolve(derivedDataPath, 'Build/Products') &&
    digest(readFileSync(proof.original)) === proof.sha256, 'Genuine current generated xctestrun proof missing');
  return proof;
}


/** Existing standalone/range invocation, never a new identity per actor. */
export function prepareIosCallerIdentity({ scenario, scenarioPath, testNumber, attemptToken, artifactsDirectory }) {
  assertIosIdentityAdmission({ testNumber, scenario });
  const runId = process.env.E2E_RUN_ID ?? 'standalone-' + attemptToken;
  check(safeId(runId), 'Invalid declared iOS invocation ID');
  if (process.env.E2E_RANGE_RUN === '1') check(process.env.E2E_IOS_IDENTITY_ROOT, 'Range iOS identity handoff missing');
  const runRoot = process.env.E2E_IOS_IDENTITY_ROOT ?? resolve(artifactsDirectory, 'runs', runId);
  const context = createIosIdentityContext({ runRoot, runId, testNumber,
    attempt: Number(process.env.E2E_TEST_ATTEMPT ?? '1'), scenarioPath, scenario, udid: EXISTING_IOS_TARGET.udid });
  const scope = process.env.E2E_RANGE_RUN === '1' && !scenario.deletionBlock
    ? requireIosIdentityBoundary(context) : prepareIosIdentityBoundary(context);
  return { context, scope };
}
