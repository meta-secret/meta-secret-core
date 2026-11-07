import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {runInNewContext} from 'node:vm';
import {parse} from '../../meta-secret/web-cli/ui/node_modules/acorn/dist/acorn.mjs';
const root = process.env.WEB_AUTH_SOURCE_ROOT ?? resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const controller = readFileSync(resolve(root,'meta-secret-core/e2eTest/Test4/orchestrator.mjs'),'utf8');
const actions = readFileSync(resolve(root,'meta-secret-core/e2eTest/shared/web-liveboard-actions.mjs'),'utf8');
const inbox = readFileSync(resolve(root,'meta-secret-core/e2eTest/shared/web-liveboard-inbox.mjs'),'utf8');
const ast = text => parse(text,{ecmaVersion:'latest',sourceType:'module'});
function declaration(text,name,optional=false){
 const hits=ast(text).body.map(n=>n.type==='ExportNamedDeclaration'?n.declaration:n).filter(n=>n?.type==='FunctionDeclaration'&&n.id.name===name);
 if(optional&&!hits.length)return '';
 assert.equal(hits.length,1,name);return text.slice(hits[0].start,hits[0].end);
}
function arrow(text){
 const parent=ast(text).body.find(n=>n.type==='FunctionDeclaration'&&n.id.name==='runIosSecondaryCreatorLifecycle');
 const hits=[];const walk=n=>{if(!n||typeof n!=='object')return;if(n.type==='VariableDeclarator'&&n.id.name==='waitForReceiverVisible')hits.push(n.init);for(const [k,v]of Object.entries(n))if(k!=='parent')Array.isArray(v)?v.forEach(walk):walk(v)};walk(parent);
 assert.equal(hits.length,1);return text.slice(hits[0].start,hits[0].end);
}
function fixture(options={}){
 const s={now:0,locked:true,credential:'fixture-existing-credential',doc:{},authClicks:0,createClicks:0,inboxClicks:0,publications:0,disposed:0,phase:true,heads:[],timeouts:[],...options};
 const context={},identity={deviceId:'web-device-fixture',deviceName:'Web fixture',credential:'fixture-existing-credential',context,origin:'https://fixture.invalid'};
 const head={kind:'recover',requestId:'request-fixture',requester:'Android fixture',subject:'Secret fixture',count:1,...options.head};
 const clock=class extends Date{static now(){return s.now}};
 const execute=(fn,arg)=>runInNewContext('('+fn.toString()+')(argument)',{argument:arg,window:{localStorage:{getItem:()=>s.credential},__METASECRET_E2E_AUTHENTICATED:s.marker??true},document:s.doc});
 const wait=async(kind,opts)=>{s.timeouts.push(opts.timeout);assert(opts.timeout>0&&opts.timeout<=120000);if(kind==='auth-visible'){s.now+=s.authWait??0;if(s.replaceDocument)s.doc={};if(s.abortAtAuthWait)s.phase=false;return;}if(kind==='auth-hidden'){if(s.locked)throw Error('authentication did not hide');return;}if(kind==='board'&&s.locked)throw Error('actual board blocked by authentication');if(kind==='board')s.now+=s.boardWait??0;};
 const fixed = (kind, effect) => async () => {
   const captured = s.doc;
   if (s.resolutionMutation === 'document') s.doc = {};
   if (s.resolutionMutation === 'phase') s.phase = false;
   return { evaluate: async fn => execute(fn, { isConnected: captured === s.doc, ownerDocument: captured }), dispose: async () => { s.handleDisposed = (s.handleDisposed ?? 0) + 1; },
     click: async opts => {
       assert(opts.timeout > 0 && opts.timeout <= (kind === 'auth' ? 30000 : 120000));
       if (opts.trial) {
         if (s.trialBoundary === kind) {
           if (s.trialMutation === 'document') s.doc = {};
           if (s.trialMutation === 'phase') s.phase = false;
           if (s.trialMutation === 'deadline') s.now = 120001;
         }
         return;
       }
       // Already-dispatched fixed-handle action: normal internal waits may
       // finish after host cancellation. Never retarget a replacement node.
       if (s.dispatchBoundary === kind) {
         if (s.dispatchMutation === 'document') s.doc = {};
         if (s.dispatchMutation === 'phase') s.phase = false;
         if (s.dispatchMutation === 'deadline') s.now = 120001;
       }
       if (captured !== s.doc) throw Error('fixed input handle detached');
       await effect(opts);
     },
   };
 };
 const auth={isVisible:async()=>s.locked,waitFor:opts=>wait('auth-'+opts.state,opts),click:async opts=>{if(opts)s.timeouts.push(opts.timeout);s.authClicks++;if(s.cancelAuth)throw Error('authentication cancelled');s.locked=false;s.now+=s.authClickCost??0;}};
 auth.elementHandle = fixed('auth', options => auth.click(options));
 const create={isVisible:async()=>false,waitFor:opts=>wait('auth-'+opts.state,opts),click:async()=>{s.createClicks++;throw Error('unexpected registration');}};
 const board={isVisible:async()=>!s.locked && !s.needsBack,or(){return this},filter(){return this},first(){return this},waitFor:opts=>wait('board',opts),click:async()=>{throw Error('unexpected navigation')}};
 board.elementHandle = fixed('back', options => { s.backClicks = (s.backClicks ?? 0) + 1; s.onBack?.(options); });
 const notificationActions={waitFor:async opts=>{s.timeouts.push(opts.timeout)}};
 const notification={waitFor:async opts=>{s.timeouts.push(opts.timeout)},getAttribute:async()=>s.inboxClicks?'actions':'request',
 locator:name=>name==='.notification-actions'?notificationActions:{click:async opts=>{if(opts)s.timeouts.push(opts.timeout);s.inboxClicks++;}, elementHandle: fixed('inbox', async opts => {s.timeouts.push(opts.timeout);s.inboxClicks++})},evaluate:async()=>head};
 const page={isClosed:()=>Boolean(s.closed),context:()=>s.wrongContext?{}:context,url:()=>s.wrongOrigin?'https://foreign.invalid/':'https://fixture.invalid/',title:async()=>'',
 evaluate:async fn=>{const result=execute(fn);if(fn.toString().includes('__METASECRET_E2E_AUTHENTICATED = true'))s.marker=true;return result},
 evaluateHandle:async()=>{const captured=s.doc;return{evaluate:async fn=>execute(fn,captured),dispose:async()=>{s.disposed++}}},
 getByRole:(_,opts)=>opts.name==='Authenticate with Passkey'?auth:opts.name==='Create Passkey'?create:{...board,isVisible:async()=>!s.locked && s.needsBack,click:async()=>{s.backClicks=(s.backClicks??0)+1;s.needsBack=false;},elementHandle:fixed('back',async()=>{s.backClicks=(s.backClicks??0)+1;s.needsBack=false;})},
 getByTestId:id=>id==='liveboard-notifications'?notification:board,
 waitForFunction:async(_fn,_arg,opts)=>{s.timeouts.push(opts.timeout);if(s.locked)throw Error('auth gate');},
 locator:selector=>selector==='.device-slot[data-device-id]'?{evaluateAll:async()=>[{id:s.wrongMember?'foreign':identity.deviceId,name:identity.deviceName}]}:
 {first(){return this},waitFor:()=>new Promise(()=>{}),innerText:async()=>'',},};
 const phase={assertActive(){if(!s.phase)throw Error('phase aborted')},web:event=>{assert.equal(event,'INCOMING_VISIBLE');s.publications++}};
 const authTarget={page:s.wrongTarget?{}:page,cdp:{send:async()=>({credentials:[]})},authenticatorId:'fixture-target'};
 const ports={Date:clock,console:{log(){}},primaryVirtualAuthenticator:authTarget,webEntryReady:()=>({waitFor:async opts=>wait('board',opts)}),writeDiagnostic(){},
 TIMEOUT:120000,observed:new WeakMap(),page,receiverIdentity:identity,receiverPhase:phase,verifyBothReceiverAlerts:true,secretName:'Secret fixture',observer:'web',testNumber:27,currentCycleForDiagnostics:22};
 const functions=[declaration(actions,'openWebLiveboard'),declaration(inbox,'validateIncomingHead'),declaration(inbox,'openWebIncoming'),
 ...['createWebReceiverActionBudget','authorizeWebReceiverAction'].map(n=>declaration(controller,n,true)),declaration(controller,'unlockWithPasskeyIfNeeded'),
 declaration(controller,'waitForStrictWebIncomingVisible'),declaration(controller,'openStrictWebIncomingAlert')].join('\n');
 const api=runInNewContext(functions+'\n({incoming:'+arrow(controller)+',authorize:typeof authorizeWebReceiverAction===\'function\'?authorizeWebReceiverAction:null})',ports);
 return{s,page,identity,phase,api};
}
// SAME actual old nested receiver arrow compiles in BEFORE. Locked-after-ready
// behavior is the intended RED: board blocked, not absent newly named API.
test('actual later Web observer reauthenticates existing target, preserves exact head and makes no decision',async()=>{
 const f=fixture();await f.api.incoming('web',null);assert.equal(f.s.authClicks,1);assert.equal(f.s.createClicks,0);assert.equal(f.s.inboxClicks,1);assert.equal(f.s.publications,1);assert.equal(f.s.disposed,1);
});
test('authenticated receiver performs no authentication gesture',async()=>{const f=fixture({locked:false});await f.api.incoming('web',null);assert.equal(f.s.authClicks,0);assert.equal(f.s.publications,1)});
for(const [name,opts,pattern]of[
 ['missing credential',{credential:null},/credential/],['foreign credential',{credential:'foreign'},/credential/],['wrong recycled target',{wrongTarget:true},/target/],['wrong context',{wrongContext:true},/target/],['wrong origin',{wrongOrigin:true},/target/],
 ['document replaced during readiness',{replaceDocument:true},/document/],['phase aborted during readiness',{abortAtAuthWait:true},/phase/],['auth cancelled',{cancelAuth:true},/cancelled/],
 ['deadline before auth input',{authWait:120001},/deadline/],['deadline after auth input',{authClickCost:120001},/deadline/],['canonical member changed',{wrongMember:true},/member/],
 ['FIFO foreign subject',{head:{subject:'foreign'}},/FIFO/],['FIFO wrong count',{head:{count:2}},/queue count/],
 ['same120 shared across auth and board',{authWait:29000,authClickCost:29000,boardWait:63000},/deadline/],
])test(name+' stops without incoming publication or decision',async()=>{
 const f=fixture(opts);await assert.rejects(f.api.incoming('web',null),pattern);assert.equal(f.s.publications,0);assert.equal(f.s.createClicks,0);
 if(['missing credential','foreign credential','wrong recycled target','wrong context','wrong origin','document replaced during readiness','phase aborted during readiness','deadline before auth input'].includes(name))assert.equal(f.s.authClicks,0);
 if(!name.startsWith('FIFO'))assert.equal(f.s.inboxClicks,0);
});
test('finite auth sublimits remain at most30 and original receiver cap stays120',async()=>{const f=fixture({authWait:10000,authClickCost:10000});await f.api.incoming('web',null);assert(f.s.timeouts.every(n=>n>0&&n<=120000));assert.equal(f.s.timeouts[0],30000);assert.equal(f.s.timeouts[1],30000)});

for (const boundary of ['auth','back','inbox']) for (const mutation of ['document','phase','deadline']) {
  test(`fresh fence after ${boundary} trial ${mutation} refuses actual input`, async () => {
    const f = fixture({ locked: boundary === 'auth', needsBack: boundary === 'back', trialBoundary: boundary, trialMutation: mutation });
    await assert.rejects(f.api.incoming('web', null), /document|phase|deadline/);
    assert.equal(f.s.publications, 0); assert.equal(f.s.createClicks, 0);
    assert.equal(boundary === 'auth' ? f.s.authClicks : boundary === 'back' ? f.s.backClicks ?? 0 : f.s.inboxClicks, 0);
  });
  test(`already dispatched ${boundary} ${mutation} never targets replacement or publishes later readiness`, async () => {
    const f = fixture({ locked: boundary === 'auth', needsBack: boundary === 'back', dispatchBoundary: boundary, dispatchMutation: mutation });
    await assert.rejects(f.api.incoming('web', null), /detached|phase|deadline/);
    assert.equal(f.s.publications, 0); assert.equal(f.s.createClicks, 0);
    const emitted = boundary === 'auth' ? f.s.authClicks : boundary === 'back' ? f.s.backClicks ?? 0 : f.s.inboxClicks;
    assert.equal(emitted, mutation === 'document' ? 0 : 1, 'post-dispatch cancellation is not physical rollback');
    if (boundary !== 'inbox') assert.equal(f.s.inboxClicks, 0);
  });
}

for (const resolutionMutation of ['document', 'phase']) test(`captured input handle released when resolver postguard rejects ${resolutionMutation}`, async () => {
  const f = fixture({ resolutionMutation });
  await assert.rejects(f.api.incoming('web', null), /document|phase/);
  assert.equal(f.s.authClicks, 0); assert.equal(f.s.createClicks, 0);
  assert.equal(f.s.inboxClicks, 0); assert.equal(f.s.publications, 0);
  assert.equal(f.s.handleDisposed, 1); assert.equal(f.s.disposed, 1);
});
