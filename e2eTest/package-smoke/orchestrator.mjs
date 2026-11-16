// Owns only this package-smoke server/browser run. Never builds or patches a package.
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve, dirname, basename, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';

const argv = process.argv.slice(2);
const arg = (name) => argv[argv.indexOf(name) + 1];
const runId = arg('--run-id'), platform = arg('--platform');
assert(runId && /^[a-zA-Z0-9-]{1,64}$/.test(runId), 'G4.RunId');
assert(['web', 'android', 'ios'].includes(platform), 'G4.Platform');
for (const key of ['--pkg', '--manifest', '--driver-bin', '--driver-sha', '--output']) assert(argv.includes(key), 'G4.RequiredArgument');
const pkgRoot = resolve(arg('--pkg')), output = resolve(arg('--output'));
const driverBin = resolve(arg('--driver-bin')), origin = 'http://127.0.0.1:3002';
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const manifest = JSON.parse(await readFile(resolve(arg('--manifest')), 'utf8'));
assert(manifest.environment === 'local' && /^[a-f0-9]{64}$/.test(manifest.sourceIdentitySha256), 'G4.LocalPackageProvenance');
const requiredFiles = ['meta_secret_wasm.js', 'meta_secret_wasm.d.ts', 'meta_secret_wasm_bg.wasm', 'meta_secret_wasm_bg.wasm.d.ts', 'package.json'];
const files = [];
for (const name of requiredFiles) {
  const entry = manifest.files.find((f) => f.path === name);
  assert(entry && sha(await readFile(resolve(pkgRoot, name))) === entry.sha256, 'G4.PackageHash');
  files.push({ path: name, sha256: entry.sha256 });
}
assert(sha(await readFile(driverBin)) === arg('--driver-sha'), 'G4.DriverHash');
const config = { runId, actor: 'A', vaultName: `g4-${runId}`, primaryName: `g4-retained-${runId}`, deleteName: `g4-delete-${runId}` };
const phases = new Set(('initialized registered join-submitted member secret-created recover1-requested recover1-approved recover1-shown recover2-requested recover2-approved second-selected second-shown delete-produced delete-approved remove3-produced remove3-approved member2 observation-fault-armed remove2-produced reconciled-unknown observation-fault-cleared reconciled-applied continued capability-negative complete').split(' '));
const hostPhases = new Set(['capability-negative', 'observation-fault-armed', 'observation-fault-cleared']);
const idKeys = new Set(['deviceId', 'passId', 'claimId', 'firstClaimId', 'requestId', 'referenceJson']);
const events = [], polls = new Set(), seen = new Set();
let browser, driver, driverExited = false, stopping = false, failure = false, transition = Promise.resolve();
let driverDone, driverCode, driverSignal, forcedKill = false, stopPromise;
let armSnapshot, operationSnapshot, referenceSources;
const exact = (raw) => JSON.parse(raw.replace(/"(?:\\.|[^"\\])*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g,
  (token) => token.startsWith('"') ? token : JSON.stringify(token)));
const stable = (value) => JSON.stringify(value, (_k, v) => v && typeof v === 'object' && !Array.isArray(v)
  ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, v[k]])) : v);
const control = async (path, body) => {
  const response = await fetch(`http://127.0.0.1:3001${path}`, { method: body ? 'POST' : 'GET',
    headers: body ? { 'Content-Type': 'application/json' } : {}, body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(5000) });
  assert(response.ok, 'G4.ControlUnavailable'); return response.json();
};
const evidence = async () => ({ runId, ...await control('/snapshot') });
function send(res, status, value) {
  if (res.destroyed || res.writableEnded) return;
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); res.end(JSON.stringify(value));
}
function flush() {
  for (const p of polls) {
    if (events.length > p.after || failure) { clearTimeout(p.timer); polls.delete(p); send(p.res, 200, { runId, cursor: events.length, events: events.slice(p.after) }); }
  }
}
const find = (phase, actor) => events.find((e) => e.phase === phase && e.actor === actor && e.status === 'ready');
function append(event) {
  assert(events.length < 128, 'G4.EventLimit');
  const entry = { cursor: events.length + 1, ...event }; events.push(entry);
  if (event.status === 'failed') failure = true; flush(); return entry;
}
async function advance(event) {
  if (!armSnapshot && find('member2', 'A') && find('member2', 'B')) {
    const actorId = find('registered', 'A')?.ids.deviceId; assert(actorId, 'G4.FaultActor');
    armSnapshot = await evidence(); await control('/fault', { enabled: true, actorId });
    append({ actor: 'host', phase: 'observation-fault-armed', status: 'ready', ids: {}, checks: {
      serverReadErrorsBeforeFault: Object.values(armSnapshot.readErrors).some((count) => count > 0),
    } });
  }
  if (event.actor === 'A' && event.phase === 'remove2-produced') {
    assert(armSnapshot, 'G4.FaultNotArmed'); operationSnapshot = await evidence();
    assert.equal(operationSnapshot.faultHits, armSnapshot.faultHits + 1, 'G4.ActualFinalReadFault');
    const ref = exact(event.ids.referenceJson); assert(ref.version === '1' && ref.kind === 'removeDevice' && ref.decisionStage, 'G4.ActualRU');
    referenceSources = [ref.requestStage, ref.decisionStage].map(stable);
    const actual = operationSnapshot.business.filter((b) => b.actorId === find('registered', 'A').ids.deviceId && b.serverOk)
      .map((b) => stable({ eventId: exact(b.eventIdJson), bodyCommitment: b.bodyCommitment }));
    for (const source of referenceSources) assert(actual.includes(source), 'G4.RealAdmittedSource');
    assert.notEqual(referenceSources[0], referenceSources[1], 'G4.DistinctRU');
  }
  if (event.actor === 'A' && ['reconciled-unknown', 'reconciled-applied', 'continued'].includes(event.phase)) {
    assert(operationSnapshot, 'G4.NoOperationSnapshot');
    const current = await evidence(), ref = exact(event.ids.referenceJson);
    assert.deepEqual([ref.requestStage, ref.decisionStage].map(stable), referenceSources, 'G4.SameRU');
    assert.equal(current.writes, operationSnapshot.writes, 'G4.NoAutomaticWireWrite');
    assert.deepEqual(current.business, operationSnapshot.business, 'G4.NoReplacementCommand');
    if (event.phase === 'reconciled-unknown') {
      assert.equal(current.faultHits, operationSnapshot.faultHits + 1, 'G4.ReconcileActualReadFault');
      assert.equal(current.observations, operationSnapshot.observations + 1, 'G4.ReadOnlyOneQuery');
      assert.equal(current.ordinaryReads, operationSnapshot.ordinaryReads, 'G4.NoOrdinarySync');
      await control('/fault', { enabled: false, actorId: find('registered', 'A').ids.deviceId });
      append({ actor: 'host', phase: 'observation-fault-cleared', status: 'ready', ids: {}, checks: {} });
    }
    if (event.phase === 'reconciled-applied') {
      assert.equal(current.observations, operationSnapshot.observations + 2, 'G4.HealthyReadOnlyOneQuery');
      assert.equal(current.ordinaryReads, operationSnapshot.ordinaryReads, 'G4.HealthyNoOrdinarySync');
    }
    if (event.phase === 'continued') {
      assert.equal(current.faultHits, operationSnapshot.faultHits + 1, 'G4.NoExtraFault');
      assert.equal(current.observations, operationSnapshot.observations + 3, 'G4.ReconcileThenExplicitContinue');
    }
  }
}
async function parse(req) {
  let bytes = 0, raw = '';
  for await (const chunk of req) { bytes += chunk.length; assert(bytes <= 16384, 'G4.BodyLimit'); raw += chunk; }
  return JSON.parse(raw);
}
function validate(body) {
  assert(body.runId === runId && ['A', 'B', 'C'].includes(body.actor) && phases.has(body.phase) && !hostPhases.has(body.phase), 'G4.PhaseIdentity');
  assert(['ready', 'failed'].includes(body.status), 'G4.PhaseStatus');
  assert(Object.keys(body).every((k) => ['runId', 'actor', 'phase', 'status', 'ids', 'checks'].includes(k)), 'G4.PhaseFields');
  const ids = body.ids ?? {}, checks = body.checks ?? {};
  assert(Object.entries(ids).every(([k, v]) => idKeys.has(k) && typeof v === 'string' && v.length <= (k === 'referenceJson' ? 4096 : 512)), 'G4.OpaqueIds');
  assert(Object.entries(checks).length <= 24 && Object.entries(checks).every(([k, v]) => /^[a-zA-Z][a-zA-Z0-9]{0,63}$/.test(k) && typeof v === 'boolean'), 'G4.SafeChecks');
  const key = `${body.actor}:${body.phase}`; if (body.status === 'ready') { assert(!seen.has(key), 'G4.DuplicatePhase'); seen.add(key); }
  return { actor: body.actor, phase: body.phase, status: body.status, ids, checks };
}
const server = createServer(async (req, res) => {
  try {
    assert(['127.0.0.1:3002', '10.0.2.2:3002', 'localhost:3002'].includes(req.headers.host), 'G4.Host');
    assert(!req.headers.origin || req.headers.origin === origin, 'G4.Origin');
    const url = new URL(req.url, origin);
    if (req.method === 'GET' && url.pathname === '/config') return send(res, 200, config);
    if (req.method === 'GET' && url.pathname === '/evidence') return send(res, 200, await evidence());
    if (req.method === 'GET' && url.pathname === '/phase') {
      const after = Number(url.searchParams.get('after'));
      assert(url.searchParams.get('runId') === runId && Number.isSafeInteger(after) && after >= 0 && after <= events.length, 'G4.Cursor');
      if (events.length > after || failure) return send(res, 200, { runId, cursor: events.length, events: events.slice(after) });
      assert(polls.size < 16, 'G4.PollLimit');
      const p = { res, after, timer: setTimeout(() => { polls.delete(p); send(res, 200, { runId, cursor: events.length, events: events.slice(after) }); }, 20000) };
      polls.add(p); res.on('close', () => { clearTimeout(p.timer); polls.delete(p); }); return;
    }
    if (req.method === 'POST' && url.pathname === '/phase') {
      const event = validate(await parse(req));
      transition = transition.then(async () => { const entry = append(event); await advance(entry); return entry; });
      return send(res, 200, await transition);
    }
    if (req.method === 'GET' && url.pathname === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store',
        'Content-Security-Policy': "default-src 'self'; connect-src 'self' http://127.0.0.1:3000; script-src 'self' 'wasm-unsafe-eval'" });
      return res.end('<!doctype html><title>G4 package ABI harness</title><script type="module" src="/wasm.mjs"></script>');
    }
    if (req.method === 'GET' && (url.pathname === '/wasm.mjs' || requiredFiles.some((n) => url.pathname === `/pkg/${n}`))) {
      const file = url.pathname === '/wasm.mjs' ? fileURLToPath(new URL('./wasm.mjs', import.meta.url)) : resolve(pkgRoot, basename(url.pathname));
      assert(file === fileURLToPath(new URL('./wasm.mjs', import.meta.url)) || file.startsWith(`${pkgRoot}${sep}`), 'G4.StaticPath');
      res.writeHead(200, { 'Content-Type': file.endsWith('.wasm') ? 'application/wasm' : file.endsWith('.json') ? 'application/json' : 'text/javascript', 'Cache-Control': 'no-store' }); return res.end(await readFile(file));
    }
    send(res, 404, { errorCode: 'RouteUnavailable' });
  } catch { failure = true; flush(); send(res, 400, { errorCode: 'HarnessUnavailable' }); }
});
const deadline = async (fn, ms = 120000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await fn()) return; if (failure || driverExited) throw new Error('G4.OwnedProcessUnavailable'); await new Promise((r) => setTimeout(r, 100)); }
  throw new Error('G4.Deadline');
};
const stop = () => stopPromise ??= (async () => {
  stopping = true;
  await browser?.close().catch(() => {});
  if (driver && !driverExited) {
    await control('/stop', {}).catch(() => {});
    await Promise.race([driverDone, new Promise((r) => setTimeout(r, 5000))]);
    if (!driverExited) {
      forcedKill = true; driver.kill('SIGTERM');
      await Promise.race([driverDone, new Promise((r) => setTimeout(r, 3000))]);
      if (!driverExited) { driver.kill('SIGKILL'); await driverDone; }
    }
  }
  for (const p of polls) { clearTimeout(p.timer); send(p.res, 503, { errorCode: 'RunClosed' }); } polls.clear();
  await new Promise((r) => server.close(r));
  return { code: driverCode ?? null, signal: driverSignal ?? null, forcedKill };
})();
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => { failure = true; void stop().finally(() => process.exit(1)); });
let success = false;
try {
  await new Promise((accept, reject) => { server.once('error', reject); server.listen(3002, '127.0.0.1', accept); });
  driver = spawn(driverBin, ['--run-id', runId, '--data-port', '3000', '--control-port', '3001'], { stdio: ['ignore', 'ignore', 'ignore'], env: { ...process.env, RUST_MIN_STACK: '33554432' } });
  driverDone = new Promise((done) => {
    driver.once('error', () => { failure = true; driverExited = true; done(); });
    driver.once('close', (code, signal) => { driverCode = code; driverSignal = signal; driverExited = true; if (!stopping) failure = true; done(); });
  });
  await deadline(async () => { try { const r = await control('/ready'); return r.ready && r.runId === runId; } catch { return false; } }, 15000);
  const untouched = await evidence(); assert.equal(untouched.writes, 0, 'G4.CleanServer');
  browser = await chromium.launch({ headless: true });
  const guardContext = await browser.newContext(), guardPage = await guardContext.newPage();
  await guardPage.goto(origin);
  const beforeGuard = await evidence();
  const checks = await guardPage.evaluate(async () => (await import('/wasm.mjs')).capabilityNegative());
  const afterGuard = await evidence();
  assert.deepEqual(afterGuard, beforeGuard, 'G4.NegativeActualServerZeroWrites');
  assert.equal(afterGuard.writes, 0, 'G4.GlobalPreInitWindow');
  await guardContext.close();
  append({ actor: 'host', phase: 'capability-negative', status: 'ready', ids: {}, checks: { ...checks, zeroServerWrites: true } });
  const actors = platform === 'web' ? ['A', 'B', 'C'] : ['B', 'C'];
  const work = actors.map(async (actor) => {
    try {
      const context = await browser.newContext(), page = await context.newPage();
      await page.goto(origin);
      return await page.evaluate(async ({ actor, runId }) => (await import('/wasm.mjs')).runActor(actor, runId), { actor, runId });
    } catch { failure = true; flush(); return false; }
  });
  await deadline(async () => ['A', 'B', 'C'].every((a) => find('complete', a)), 600000);
  assert((await Promise.all(work)).every(Boolean), 'G4.BrowserActorResult');
  assert(find('second-shown', 'B')?.checks.sameOwnedAcrossAwait, 'G4.OwnedSecondUnreached');
  assert(find('continued', 'A') && operationSnapshot, 'G4.ProducerUnreached'); success = true;
} catch { failure = true; }
finally {
  const actual = await evidence().catch(() => undefined);
  const driverDrain = await stop();
  success = success && !failure && driverDrain.code === 0 && driverDrain.signal === null && !driverDrain.forcedKill;
  await mkdir(output, { recursive: true });
  await writeFile(resolve(output, `G4-${runId}-${platform}.json`), JSON.stringify({ runId, platform, status: success ? 'passed' : 'failed', sourceIdentitySha256: manifest.sourceIdentitySha256,
    packageFiles: files, driverSha256: arg('--driver-sha'), driverDrain, events, evidence: actual, qualification: 'package ABI component smoke; not application UI E2E' }, null, 2));
}
console.log(success ? 'G4 package smoke passed' : 'G4 package smoke failed');
if (!success) process.exitCode = 1;
