// Served as a browser module. Every business action uses the actual generated package.
import init, * as pkg from '/pkg/meta_secret_wasm.js';
const assert = (ok) => { if (!ok) throw new Error('G4.AssertUnavailable'); };
const pause = () => new Promise((r) => setTimeout(r, 120));
const VALUE = 'G4 package fixture value'; // Compared only in-process; never reported.
const DIAGNOSTIC_LINE_LIMIT = 283; // Bound to this exact source body.
const REQUIRED = ['cluster_distribution_checked', 'recover_checked', 'decide_join_request',
  'decide_remove_member', 'delete_secret_checked', 'remove_member_checked',
  'reconcile_producer_receipt', 'continue_local_remove_checked', 'show_recovered'];
let writes = 0;
let dataCalls = 0;
const realFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = (...args) => { if (String(args[0]).includes('/meta_request')) dataCalls++; return realFetch(...args); };
for (const key of ['add', 'put', 'delete', 'clear']) {
  const original = IDBObjectStore.prototype[key];
  IDBObjectStore.prototype[key] = function (...args) { writes++; return original.apply(this, args); };
}
// Quote numeric tokens before JSON.parse: opaque u64 values never enter JS Number.
export const exact = (raw) => JSON.parse(raw.replace(/"(?:\\.|[^"\\])*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g,
  (token) => token.startsWith('"') ? token : JSON.stringify(token)));
export function field(raw, wanted) {
  let i = 0;
  const ws = () => { while (/\s/.test(raw[i] ?? '') && i < raw.length) i++; };
  const value = () => {
    const start = i;
    if (raw[i] === '"') { i++; while (i < raw.length) { if (raw[i++] === '\\') i++; else if (raw[i - 1] === '"') break; } }
    else if ('[{'.includes(raw[i])) {
      const end = raw[i++] === '{' ? '}' : ']'; ws();
      while (raw[i] !== end && i < raw.length) { if (raw[i] === ':' || raw[i] === ',') i++; else value(); ws(); } i++;
    } else { while (i < raw.length && !/[,}\]\s]/.test(raw[i])) i++; }
    return raw.slice(start, i);
  };
  ws(); assert(raw[i++] === '{'); ws();
  while (i < raw.length && raw[i] !== '}') {
    const key = JSON.parse(value()); ws(); assert(raw[i++] === ':'); ws();
    const result = value(); if (key === wanted) return result; ws(); if (raw[i] === ',') i++; ws();
  }
  return undefined;
}
export function reference(raw) {
  const stage = field(raw, 'requestStage'), decision = field(raw, 'decisionStage');
  const parts = [`"version":${field(raw, 'version')}`, `"kind":${field(raw, 'kind')}`,
    `"requestStage":${field(stage, 'source')}`];
  const id = field(raw, 'requestId'); if (id !== undefined) parts.push(`"requestId":${id}`);
  if (decision && exact(decision).state === 'attempted') parts.push(`"decisionStage":${field(decision, 'source')}`);
  return `{${parts.join(',')}}`;
}
const receipt = (raw, kind) => { const r = exact(raw); assert(r.kind === kind && ['queued', 'unknown'].includes(r.phase)); return r; };
const producer = (raw, kind) => { const r = receipt(raw, kind); assert(r.version === '1' && r.requestStage?.source && !('success' in r)); return r; };
const sourcePair = (raw) => `${field(raw, 'requestStage')}|${field(raw, 'decisionStage') ?? ''}`;
async function createManager(namespace, actor) {
  if (typeof namespace.WasmApplicationManager.init_wasm_with_device_and_environment !== 'function'
    || !REQUIRED.every((key) => typeof namespace.WasmApplicationManager.prototype[key] === 'function')) {
    throw new Error('G4.UnsupportedPackage');
  }
  const master = namespace.MasterKeyManager.from_pure_sk(namespace.MasterKeyManager.generate_sk());
  return namespace.WasmApplicationManager.init_wasm_with_device_and_environment(master, `G4-${actor}`, 'Web', 'local');
}
export async function capabilityNegative() {
  await init();
  assert(REQUIRED.every((key) => typeof pkg.WasmApplicationManager.prototype[key] === 'function'));
  const calls = { init: 0, legacy: 0, producer: 0 };
  const legacy = new Set(['delete_secret', 'remove_member_from_vault', 'recover_js']);
  const prototype = new Proxy(pkg.WasmApplicationManager.prototype, { get(target, key) {
    if (key === 'delete_secret_checked') return undefined;
    const actual = Reflect.get(target, key);
    if (typeof actual !== 'function') return actual;
    return function (...args) {
      if (legacy.has(key)) calls.legacy++; else if (REQUIRED.includes(key)) calls.producer++;
      return actual.apply(this, args);
    };
  } });
  const facade = { ...pkg, WasmApplicationManager: { prototype,
    init_wasm_with_device_and_environment(...args) {
      calls.init++; return pkg.WasmApplicationManager.init_wasm_with_device_and_environment(...args);
    } } };
  const before = writes, network = dataCalls; let rejected = false;
  try { const unexpected = await createManager(facade, 'probe'); unexpected.free(); }
  catch (error) { rejected = error.message === 'G4.UnsupportedPackage'; }
  assert(rejected && calls.init === 0 && calls.legacy === 0 && calls.producer === 0 && writes === before && dataCalls === network);
  return { zeroInit: true, zeroLegacy: true, zeroProducer: true, zeroLocalWrites: true, zeroNetwork: true };
}

export async function runActor(actor, runId) {
  let manager, phase = 'initialized', cursor = 0, checkpoint = 'checkpointInit';
  const events = [];
  const failureChecks = (error) => {
    const kind = error instanceof TypeError ? 'errorTypeError'
      : error instanceof DOMException ? 'errorDomException'
      : error instanceof WebAssembly.RuntimeError ? 'errorWasmRuntime'
      : error instanceof Error ? 'errorError' : 'errorOther';
    const checks = { actorFailed: true, [checkpoint]: true, [kind]: true };
    if (error instanceof Error || error instanceof DOMException) {
      const known = new Map([['G4.AssertUnavailable', 'knownAssert'], ['G4.PhaseDeadline', 'knownPhaseDeadline'],
        ['G4.StateDeadline', 'knownStateDeadline'], ['Vault synchronization failed', 'knownVaultSync'],
        ['Invalid response type', 'knownResponseType'], ['Sign up is not allowed in local state', 'knownSignupLocal'],
        ['Sign up is not allowed in pending state', 'knownSignupPending'], ['Sign up is not allowed in declined state', 'knownSignupDeclined'],
        ['Sign up is not allowed in vault state', 'knownSignupMember']]);
      const code = known.get(error.message); if (code) checks[code] = true;
      if (typeof error.stack === 'string') {
        const lines = new Set();
        for (const frame of error.stack.split('\n').slice(1)) {
          const match = /^\s*at (?:[^(]* \()?http:\/\/127\.0\.0\.1:3002\/wasm\.mjs:(\d+):\d+\)?\s*$/.exec(frame);
          if (!match) continue;
          const line = Number(match[1]);
          if (Number.isSafeInteger(line) && line > 0 && line <= DIAGNOSTIC_LINE_LIMIT) lines.add(line);
          if (lines.size === 4) break;
        }
        for (const line of lines) checks[`wasmLine${line}`] = true;
      }
    }
    return checks;
  };
  const emit = async (p, ids = {}, checks = {}) => {
    phase = p; checkpoint = 'checkpointEmit';
    const response = await fetch('/phase', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ runId, actor, phase: p, status: 'ready', ids, checks }) });
    assert(response.ok); return response.json();
  };
  const wait = async (p, who) => {
    checkpoint = 'checkpointWaitPeer';
    const end = Date.now() + 120000;
    while (Date.now() < end) {
      const found = events.find((e) => e.phase === p && (!who || e.actor === who));
      if (found) { assert(found.status === 'ready'); return found; }
      const response = await fetch(`/phase?runId=${encodeURIComponent(runId)}&after=${cursor}`);
      assert(response.ok); const data = await response.json(); cursor = data.cursor; events.push(...data.events);
      assert(!events.some((e) => e.status === 'failed'));
    }
    throw new Error('G4.PhaseDeadline');
  };
  const snapshot = async () => {
    checkpoint = 'checkpointSnapshotGetState';
    const state = await manager.get_state();
    checkpoint = 'checkpointSnapshotIsVault';
    if (!state.is_vault()) { state.free(); return undefined; }
    checkpoint = 'checkpointSnapshotAsVault';
    const vault = state.as_vault();
    checkpoint = 'checkpointSnapshotIsMember';
    if (!vault.is_member()) { vault.free(); state.free(); return undefined; }
    checkpoint = 'checkpointSnapshotAsMember';
    const member = vault.as_member();
    checkpoint = 'checkpointSnapshotProjection';
    return { state, vault, member, live: exact(member.liveboard_state_json()),
      free() { member.free(); vault.free(); state.free(); } };
  };
  const poll = async (predicate) => {
    const end = Date.now() + 120000;
    while (Date.now() < end) { const s = await snapshot(); if (s && predicate(s)) return s; s?.free(); await pause(); }
    throw new Error('G4.StateDeadline');
  };
  const localReady = (s, id) => s.live.secrets.some((secret) => secret.secretId === id
    && secret.primaryAction === 'showLocal' && secret.readiness === 'ready');
  const members = (s) => {
    checkpoint = 'checkpointMemberVaultData'; const v = s.member.vault_data();
    checkpoint = 'checkpointMemberList'; const m = v.members(); const n = m.length;
    checkpoint = 'checkpointMemberFree'; for (const item of m) item.free(); v.free(); return n; };
  const ownedDevice = async (id) => {
    const s = await poll((x) => x.live.removableDeviceIds.includes(id));
    const v = s.member.vault_data(); const all = v.members(); let selected;
    for (const m of all) { const u = m.user_data, d = u.device, key = d.device_id;
      if (key.wasm_id_str() === id) selected = key; else key.free(); d.free(); u.free(); m.free(); }
    v.free(); s.free(); assert(selected); return selected;
  };
  const approveRecovery = async (id) => {
    const s = await poll((x) => x.live.operations.some((o) => o.requestId === id && o.canApprove && o.kind === 'recover'));
    const claim = s.member.recovery_claim_by_id(id); assert(claim); s.free();
    try { await manager.accept_recover(claim); await manager.sync_now(); } finally { claim.free(); }
  };
  try {
    checkpoint = 'checkpointInit'; await init();
    await wait('capability-negative', 'host');
    checkpoint = 'checkpointCreateManager'; manager = await createManager(pkg, actor);
    checkpoint = 'checkpointInitialState';
    const initial = await manager.get_state(), device = initial.device_id();
    checkpoint = 'checkpointInitialIdentity';
    const deviceId = device.wasm_id_str(); device.free(); initial.free();
    await emit('initialized', { deviceId });
    checkpoint = 'checkpointConfig';
    const config = await (await fetch('/config')).json(); assert(config.runId === runId);
    if (actor !== 'A') await wait('registered', 'A');
    if (actor === 'C') await wait('member', 'B');
    checkpoint = 'checkpointGenerateCreds'; await manager.generate_user_creds(config.vaultName);
    checkpoint = 'checkpointSignUp'; const signed = await manager.sign_up();
    checkpoint = 'checkpointRegisteredIdentity';
    const registered = signed.device_id(); assert(registered.wasm_id_str() === deviceId); registered.free(); signed.free();
    if (actor === 'A') {
      await emit('registered', { deviceId });
      for (const peer of ['B', 'C']) {
        const joined = await wait('join-submitted', peer);
        const s = await poll((x) => x.live.operations.some((o) => o.kind === 'join' && o.requesterId === joined.ids.deviceId && o.canApprove));
        const op = s.live.operations.find((o) => o.kind === 'join' && o.requesterId === joined.ids.deviceId && o.canApprove); s.free();
        checkpoint = 'checkpointJoinDecision';
        receipt(await manager.decide_join_request(JSON.stringify({ requestId: op.requestId, accepted: true })), 'join');
        await manager.sync_now();
      }
      const three = await poll((s) => members(s) === 3); three.free(); await emit('member');
    } else { await emit('join-submitted', { deviceId }); const s = await poll((x) => members(x) === (actor === 'B' ? 2 : 3)); s.free(); await emit('member'); }
    const pass = pkg.MetaPasswordId.build_from_str(config.primaryName);
    const deleted = pkg.MetaPasswordId.build_from_str(config.deleteName);
    if (actor === 'C') { await wait('continued', 'A'); await emit('complete'); pass.free(); deleted.free(); return true; }
    if (actor === 'A') {
      for (const name of [config.primaryName, config.deleteName]) {
        const plain = new pkg.PlainPassInfo(name, VALUE);
        try { receipt(await manager.cluster_distribution_checked(plain), 'create'); } finally { plain.free(); }
      }
      await manager.sync_now(); await emit('secret-created', { passId: pass.id_str() });
      const first = receipt(await manager.recover_checked(pass), 'recover'); assert(first.claimId);
      await manager.sync_now(); await emit('recover1-requested', { claimId: first.claimId });
      await wait('recover1-approved', 'B');
      const s = await poll((x) => x.live.operations.some((o) => o.requestId === first.claimId && o.clientStatus === 'accepted'));
      const claim = s.member.recovery_claim_by_id(first.claimId); assert(claim); s.free();
      try { assert(await manager.show_recovered(claim) === VALUE); } finally { claim.free(); }
      await emit('recover1-shown', { firstClaimId: first.claimId });
      const second = await wait('recover2-requested', 'B'); await approveRecovery(second.ids.claimId);
      await emit('recover2-approved', { claimId: second.ids.claimId }); await wait('second-shown', 'B');
      const delRaw = await manager.delete_secret_checked(deleted), del = producer(delRaw, 'deleteSecret');
      await emit('delete-produced', { requestId: del.requestId, referenceJson: reference(delRaw) });
      await wait('delete-approved', 'B'); const clean = await poll((x) => !x.live.secrets.some((p) => p.secretId === deleted.id_str())); clean.free();
      const remote = await wait('remove3-produced', 'B');
      const pending = await poll((x) => x.live.operations.some((o) => o.kind === 'removeDevice' && o.requestId === remote.ids.requestId && o.canApprove)); pending.free();
      receipt(await manager.decide_remove_member(JSON.stringify({ requestId: remote.ids.requestId, accepted: true })), 'removeDevice');
      await emit('remove3-approved'); const two = await poll((x) => members(x) === 2 && localReady(x, pass.id_str())); two.free();
      assert(await manager.show_local_secret(pass) === VALUE);
      await emit('member2', {}, { localPrimaryReady: true, plaintextMatchesInRam: true });
      await wait('observation-fault-armed', 'host');
      const peer = await wait('initialized', 'B'), target = await ownedDevice(peer.ids.deviceId);
      let raw; try { raw = await manager.remove_member_checked(target); } finally { target.free(); }
      const original = producer(raw, 'removeDevice'), ref = reference(raw);
      assert(original.phase === 'unknown' && original.decisionStage?.state === 'attempted');
      await emit('remove2-produced', { referenceJson: ref }, {
        materialReady: original.materialStage === 'ready', materialUnknown: original.materialStage === 'unknown',
        materialPending: original.materialStage === 'pending', canonicalApplied: original.canonicalState === 'applied',
        canonicalUnknown: original.canonicalState === 'unknown', knownOperationUnavailable: original.errorCode === 'OperationUnavailable',
      });
      const localBefore = writes, uncertain = await manager.reconcile_producer_receipt(ref);
      assert(producer(uncertain, 'removeDevice').phase === 'unknown' && sourcePair(reference(uncertain)) === sourcePair(ref) && writes === localBefore);
      await emit('reconciled-unknown', { referenceJson: reference(uncertain) }, { zeroLocalWrites: true });
      await wait('observation-fault-cleared', 'host');
      const observed = await manager.reconcile_producer_receipt(ref);
      assert(producer(observed, 'removeDevice').canonicalState === 'applied' && sourcePair(reference(observed)) === sourcePair(ref) && writes === localBefore);
      await emit('reconciled-applied', { referenceJson: reference(observed) }, { zeroLocalWrites: true });
      const continued = await manager.continue_local_remove_checked(ref);
      const final = producer(continued, 'removeDevice');
      assert(final.phase === 'queued' && final.canonicalState === 'applied' && final.materialStage === 'ready'
        && ['ready', 'done', 'settled'].includes(final.readiness) && sourcePair(reference(continued)) === sourcePair(ref));
      assert(await manager.show_local_secret(pass) === VALUE);
      await emit('continued', { referenceJson: reference(continued) }); await emit('complete');
    } else {
      await wait('secret-created', 'A'); const first = await wait('recover1-requested', 'A');
      await approveRecovery(first.ids.claimId); await emit('recover1-approved', { claimId: first.ids.claimId });
      await wait('recover1-shown', 'A');
      const second = receipt(await manager.recover_checked(pass), 'recover'); assert(second.claimId !== first.ids.claimId);
      await manager.sync_now(); await emit('recover2-requested', { claimId: second.claimId });
      await wait('recover2-approved', 'A');
      const s = await poll((x) => x.live.operations.some((o) => o.requestId === second.claimId && o.clientStatus === 'accepted')
        && x.live.operations.some((o) => o.requestId === first.ids.claimId && o.clientStatus === 'done'));
      const firstOwned = s.member.recovery_claim_by_id(first.ids.claimId), secondOwned = s.member.recovery_claim_by_id(second.claimId);
      assert(firstOwned && secondOwned && secondOwned.wasm_id_str() === second.claimId && firstOwned.wasm_id_str() !== secondOwned.wasm_id_str());
      assert(!s.member.recovery_claim_by_id('invalid-claim-key')); firstOwned.free(); s.free();
      try { await emit('second-selected', { firstClaimId: first.ids.claimId, claimId: second.claimId });
        assert(await manager.show_recovered(secondOwned) === VALUE); assert(secondOwned.wasm_id_str() === second.claimId);
      } finally { secondOwned.free(); }
      await emit('second-shown', { claimId: second.claimId }, { sameOwnedAcrossAwait: true });
      const del = await wait('delete-produced', 'A');
      const d = await poll((x) => x.live.operations.some((o) => o.requestId === del.ids.requestId && o.canApprove)); d.free();
      await manager.decide_delete_secret(del.ids.requestId, true); await manager.sync_now(); await emit('delete-approved');
      const peer = await wait('initialized', 'C'), target = await ownedDevice(peer.ids.deviceId);
      let raw; try { raw = await manager.remove_member_checked(target); } finally { target.free(); }
      const remote = producer(raw, 'removeDevice'); assert(remote.requestId);
      await emit('remove3-produced', { requestId: remote.requestId, referenceJson: reference(raw) });
      await wait('remove3-approved', 'A'); const two = await poll((x) => members(x) === 2 && localReady(x, pass.id_str())); two.free();
      assert(await manager.show_local_secret(pass) === VALUE);
      await emit('member2', {}, { localPrimaryReady: true, plaintextMatchesInRam: true });
      await wait('continued', 'A'); await emit('complete');
    }
    pass.free(); deleted.free(); return true;
  } catch (error) {
    await fetch('/phase', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ runId, actor, phase, status: 'failed', checks: failureChecks(error) }) }).catch(() => {});
    return false;
  } finally { manager?.free(); }
}
