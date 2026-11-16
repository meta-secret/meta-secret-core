// Private Stage5a bank. Root alone executes. No application module imports.
import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { runInNewContext } from 'node:vm';
// Public F1 bank always reads current owning sources; private before/after replay stays in Root artifacts.
const selector = fs.readFileSync(new URL('./ui-e2e-range-selection.mjs', import.meta.url), 'utf8');
const runner = fs.readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
const api = runInNewContext(selector.replaceAll('export ', '') + '\n({parseUiE2eRangeOptions,selectUiE2eCommand})');
const parse = args => api.parseUiE2eRangeOptions(args);
const args = ['--from', '9', '--to', '9', '--diagnostic-block', '2', '--run-id', 'new-diagnostic'];
const plain = x => JSON.parse(JSON.stringify(x));
const exact = 'node Test9/orchestrator.mjs scenario-block2.json --exit-on-success';
const deny = name => () => assert.fail(`Closed port reached: ${name}`);
function body(name) {
  const re = new RegExp(`^(?:export )?(?:async )?function ${name}\\(`, 'm');
  const start = runner.search(re); assert(start >= 0, `Actual function ${name} exists`);
  const end = runner.indexOf('\n}\n', start); assert(end > start);
  return runner.slice(start, end + 2).replace(/^export /, '');
}
function actual(name, ports) { return runInNewContext(body(name) + `\n${name};`, ports); }

// Noflag controls MUST execute first; original feature rejection below is separate causal RED.
test('00 noflag default and priority controls', () => {
  assert.deepEqual(plain(parse([])), { from: 1, to: 50, runId: null, preserveEvidence: false, pauseAfterEachCase: false, help: false });
  assert.equal(api.selectUiE2eCommand(9, {'test-9': 'one', 'test-9:blocks': 'all'}, true), 'test-9:blocks');
  assert.equal(api.selectUiE2eCommand(18, {}, false), null);
  assert.equal(parse(['--from=51', '--to=51']).to, 51);
});
test('01 scoped CLI-contract RED: original CLI must accept diagnostic route', () => {
  // Original is expected to throw Unknown option: --diagnostic-block HERE.
  // Not an absent API, extraction, test setup or SDK failure.
  const value = parse(args);
  assert.equal(value.diagnosticBlock, 2); assert.equal(value.preserveEvidence, true);
});
test('diagnostic argument matrix refuses malformed, foreign, duplicate and unsupported requests', () => {
  const bad = ['', '0', '-1', '2.0', '02', '2e0', 'NaN', 'Infinity', '3', '1'];
  for (const value of bad) assert.throws(() => parse(args.map((x,i) => i === 5 ? value : x)));
  for (const tail of [['--diagnostic-block','2'], ['--diagnostic-block=2'], ['--from','9'], ['--from=9'],
    ['--to','9'], ['--run-id','other'], ['--resume'], ['--block','2'], ['--full50']]) assert.throws(() => parse([...args,...tail]));
  for (const range of [[8,8], [9,10], [1,50], [50,50]]) assert.throws(() => parse(['--from',String(range[0]),'--to',String(range[1]),...args.slice(4)]));
  assert.throws(() => parse(args.slice(0,6))); assert.throws(() => parse(args.slice(0,5)));
  assert.equal(parse(['--from=9','--to=9','--diagnostic-block=2','--run-id=new']).preserveEvidence, true);
});
test('exact owning command only; no fallback for missing, replaced or foreign command', () => {
  assert.equal(api.selectUiE2eCommand(9, {'test-9:block2':exact}, true, 2), 'test-9:block2');
  for (const scripts of [{}, {'test-9:blocks':'all'}, {'test-9:block2':'node foreign.mjs'}, {'test-9:block2':exact+' --foreign'}])
    assert.throws(() => api.selectUiE2eCommand(9,scripts,true,2));
  assert.throws(() => api.selectUiE2eCommand(9,{'test-9:block2':exact},false,2));
});
function selected() {
  const e2eRoot = '/controlled/e2e';
  const scenarios = [1,2,3].map(n => ({path:resolve(e2eRoot,`Test9/scenario-block${n}.json`),value:{block:n}}));
  const resolveRange = actual('resolveRange', {resolve,e2eRoot, resolveTestCommand:()=>'test-9:block2', discoverTestScenarios:()=>scenarios});
  return {e2eRoot, scenarios, resolveRange};
}
test('actual resolveRange filters exact original scenario before caller preparation', () => {
  const {resolveRange} = selected(); const result = resolveRange({from:9,to:9,diagnosticBlock:2});
  assert.equal(result.entries.length,1); assert.deepEqual(plain(result.entries[0].scenarios.map(s=>s.value.block)),[2]);
  for (const values of [[],[{path:'/foreign/scenario-block2.json',value:{}}]]) {
    const fn = actual('resolveRange',{resolve,e2eRoot:'/controlled/e2e',resolveTestCommand:()=>'test-9:block2',discoverTestScenarios:()=>values});
    assert.throws(()=>fn({from:9,to:9,diagnosticBlock:2}), /canonical|scenario/);
  }
});
function mainFixture(directory, options, overrides={}) {
  const events=[]; const {resolveRange}=selected();
  const ports={resolve,testsRoot:directory,parseOptions:()=>options,resolveRange,
    process:{argv:['node','range'],env:{},version:'controlled',stdout:{write(){} }},
    mkdirSync:fs.mkdirSync,existsSync:fs.existsSync,readFileSync:(path,...rest)=>String(path).startsWith('/controlled/e2e/')?Buffer.from('original-scenario-fixture'):fs.readFileSync(path,...rest),
    sha256:()=> 'fixture-hash',collectSensitiveValues:()=>events.push('redaction'),
    RunArtifactCollector:class { constructor(){events.push('collector'); this.summaryPath='log';} writeEvent(){} },
    readFreeDiskGiB:()=>{events.push('probe');return 64;},platform:()=>'',release:()=>'',arch:()=>'',gitSnapshot:()=>({}),
    writeManifest(){events.push('manifest');},executeRange:async value=>{events.push('execute');return value;},
    resolveDeletionAuthProfile:deny('Test50'),createRunId:deny('implicit identity'),...overrides};
  return {events,main:actual('main',ports)};
}
test('actual main atomic fresh reservation rejects every existing identity before collector/probe', async t=>{
  const dir=fs.mkdtempSync(resolve(tmpdir(),'diagnostic-bank-')); t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const options={from:9,to:9,runId:'new',diagnosticBlock:2};
  for (const kind of ['empty','manifestless','sealed','supporting','file','symlink']) {
    const target=resolve(dir,'new');
    if(kind==='file') fs.writeFileSync(target,'');
    else if(kind==='symlink') fs.symlinkSync(dir,target);
    else {fs.mkdirSync(target);if(kind!=='empty')fs.writeFileSync(resolve(target,kind==='sealed'?'run.json':'evidence'), '{}');}
    const f=mainFixture(dir,options); await assert.rejects(f.main(),/EEXIST/);assert.deepEqual(f.events,[]);
    fs.rmSync(target,{recursive:true,force:true});
  }
  const f=mainFixture(dir,options); const results=await Promise.allSettled([f.main(),f.main()]);
  assert.equal(results.filter(x=>x.status==='fulfilled').length,1);assert.equal(results.filter(x=>x.status==='rejected'&&x.reason.code==='EEXIST').length,1);
  const value=results.find(x=>x.status==='fulfilled').value;
  assert.equal(value.manifest.fullAcceptance,false);assert.deepEqual(plain(value.manifest.tests),[]);
  assert.deepEqual(plain(value.entries[0].scenarios.map(s=>s.value.block)),[2]);
});
test('normal resume rejects diagnostic manifest before collectors',async t=>{
  const dir=fs.mkdtempSync(resolve(tmpdir(),'diagnostic-resume-'));t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  fs.mkdirSync(resolve(dir,'old'));fs.writeFileSync(resolve(dir,'old/run.json'),JSON.stringify({scope:{kind:'diagnostic-block'}}));
  const f=mainFixture(dir,{from:9,to:9,runId:'old'});await assert.rejects(f.main(),/cannot grant range resume credit/);assert.deepEqual(f.events,[]);
});
function execution(status, interrupted=null, restoreFailure=false, diagnostic=true) {
  const events=[], output=[]; const {resolveRange}=selected();const entries=resolveRange({from:9,to:9,...(diagnostic?{diagnosticBlock:2}:{})}).entries;
  const manifest={tests:[],status:'running',...(diagnostic?{scope:{kind:'diagnostic-block'},diagnostics:[],fullAcceptance:false}:{})};
  const check = values => assert.deepEqual(plain(values.map(x=>x.scenario?.block??x.block)),diagnostic?[2]:[1,2,3]);
  const collector={writeEvent:line=>output.push(line),close:async()=>events.push('close'),redactor:{redactText:x=>x}};
  const ports={resolve,coreRoot:'/core',composeRoot:'/compose',Date,AggregateError,E2E_BUILD_PROFILE:{},interruptedBy:interrupted,
    assertExistingDeviceRun:({scenarios})=>check(scenarios),createScenarioRuntimeProfile:x=>x,
    measurePreparationInputs:({scenarios})=>{check(scenarios);events.push('measure');return{};},planPreparationBudget:()=>({}),
    readPreparationObservation:deny('external observation'),runPreparationCommand:deny('process'),
    createPreparationResourceBudget:()=>({}),createPreparationMetrics:()=>({measure:async(_name,op)=>op()}),
    writeFileSync:deny('metrics write'),createRunnerAudit:async()=>{events.push('F1');return{};},writeManifest(){},
    assertAcceptedRunnerLineage(){events.push('lineage');},validateRunnerAudit:()=>({}),batchEnvironmentPaths:()=>({}),
    createBatchEnvironmentGuard:()=>({restore:async()=>{events.push('restore');if(restoreFailure)throw Error('restore refusal');}}),
    createBatchProductPreparation:({scenarios})=>{check(scenarios);events.push('products-bound');return{};},
    readFreeDiskGiB:()=>64,diskThresholdGiB:15,postCleanupMinimumGiB:12,announce(){},reportTestStage(){},
    completedRangeEntry:()=>false,deletionEntryComplete:r=>r.status==='passed',
    runEntry:async(_entry,run)=>{events.push('actor');if(diagnostic)assert.equal(run.preserveEvidence,true);return{test:9,status,exitCode:status==='passed'?0:1,signal:null};},
    stopRunnerOwnedRuntimes:async()=>events.push('stop'),releaseHealthyRunnerOwnedRuntimes:deny('release'),
    process:{stdout:{write:x=>output.push(x)},stderr:{write:x=>output.push(x)}}};
  ports.finishRangeOwnership=actual('finishRangeOwnership',ports);
  return {manifest,events,output,run:()=>actual('executeRange',ports)({entries,options:{from:9,to:9,preserveEvidence:true,...(diagnostic?{diagnosticBlock:2}:{})},runId:'new',runDirectory:'/controlled',manifestPath:'/controlled/run.json',manifest,collector})};
}
test('actual execution success/failure/interruption never grants whole-test or cursor credit',async()=>{
  for(const [status,signal] of [['passed',null],['failed',null],['passed','SIGINT']]) {
    const f=execution(status,signal);await f.run();assert.deepEqual(plain(f.manifest.tests),[]);
    assert.equal(f.manifest.pendingTest,null);assert.equal(f.manifest.fullAcceptance,false);
    assert(!f.output.join('').includes('TEST SUCCESS'));assert(!f.output.join('').includes('BUSINESS PASS'));
    assert.deepEqual(f.events.slice(-2),['restore','close']);
    assert.equal(f.manifest.status,signal?'diagnostic-interrupted':status==='passed'?'diagnostic-completed':'diagnostic-failed');
  }
});
test('actual restoration failure overrides scoped success',async()=>{
  const f=execution('passed',null,true);await assert.rejects(f.run(),/restore refusal/);assert.equal(f.manifest.status,'restore-failed');
});
test('actual normal execution preserves full scenario inventory and success',async()=>{
  const f=execution('passed',null,false,false);assert.equal(await f.run(),0);assert.equal(f.manifest.status,'completed');assert.equal(f.manifest.tests.length,1);
});
test('completedRangeEntry cannot promote forged passed diagnostic',()=>{
  const fn=actual('completedRangeEntry',{deletionEntryComplete:()=>true});
  assert.equal(fn({test:9,status:'passed',scope:{kind:'diagnostic-block'},fullAcceptance:false},'real-os','id'),false);
});
test('actual runEntry independently forces child evidence without caller flag',async()=>{
  let environment;const sentinel=Error('closed actor spawn reached');
  const collector={runDirectory:'/controlled',startTest:()=>'/controlled/log',writeEvent(){},redactor:{redactText:x=>x,addSensitiveValues(){}},finishTest:async()=>{}};
  const ports={Date,resolve,e2eRoot:'/e2e',rangeExecutionServices:{},interruptedBy:null,
    process:{env:{},stdout:{write(){}},stderr:{write(){}}},assertExistingDeviceRun(){},assertIosIdentityAdmission(){},
    createScenarioRuntimeProfile:()=>({kind:'ui'}),reportTestStage(){},updateReadinessReset(){},
    createIosIdentityContext:deny('iOS identity for no-iOS fixture'),prepareIosIdentityBoundary:deny('iOS fixture')};
  await assert.rejects(actual('runEntry',ports)({testNumber:9,script:'test-9:block2',scenarios:[{path:'/original',value:{}}]},
    {attempt:1,runId:'new',collector,preserveEvidence:false,diagnosticScope:{kind:'diagnostic-block'}},
    {environmentGuard:{prepare:async()=>{}},batchProducts:{prepare:async()=>'/controlled/products',nativePreparation:()=>({})}},
    {prepareScenarioPlatforms:async()=>({platforms:[],reportPath:'/controlled/readiness'}),resetE2eScenarioState:async()=>({}),
      startPlatformLivenessMonitor:()=>()=>{},discoverAttemptArtifacts:()=>[],spawn:(_command,_args,options)=>{environment=options.env;throw sentinel;}}),
    error=>error===sentinel);
  assert.equal(environment.E2E_PRESERVE_EVIDENCE,'1');assert.equal(environment.E2E_RANGE_RUN,'1');
});

// Review0a54fe24 changed-path additions; original frozen bank prefix byte-exact.
test('actual diagnostic legacy-runtime cleanup failure rejects after restore and collector close', async()=>{
  for (const diagnostic of [false,true]) {
    const events=[], error=Error('closed legacy-runtime stop refusal');
    const manifest={tests:[],status:diagnostic?'diagnostic-completed':'completed',runtimeOwnership:[{platform:'legacy',serial:'closed-existing-runtime'}],
      ...(diagnostic?{scope:{kind:'diagnostic-block'},fullAcceptance:false}:{})};
    const ports={Date,AggregateError,process:{stdout:{write(){}},stderr:{write(){}}},writeManifest(){events.push('manifest');},
      stopRunnerOwnedRuntimes:async()=>{events.push('stop');throw error;},releaseHealthyRunnerOwnedRuntimes:deny('runtime release')};
    const call=()=>actual('finishRangeOwnership',ports)({manifest,manifestPath:'/controlled/run.json',
      collector:{writeEvent(){},close:async()=>events.push('close')},environmentGuard:{restore:async()=>events.push('restore')},
      strictDeletion:false,retainHealthyOnPause:false});
    if(diagnostic) await assert.rejects(call(),e=>e===error);
    else await call();
    assert.deepEqual(events.filter(x=>x!=='manifest'),['stop','restore','close']);
    assert.equal(manifest.environmentRestoration.status,'passed');
    assert.equal(manifest.runtimeOwnership.length,1,'Failed owned cleanup retains runtime identity');
    assert.equal(manifest.status,diagnostic?'owned-runtime-cleanup-failed':'completed');
  }
});
test('actual runEntry successful closed child emits diagnostic-completed collector status',async()=>{
  const events=[], finishes=[];let environment;
  const collector={runDirectory:'/controlled',startTest:()=>'/controlled/log',writeEvent(){},writeOutput:deny('unexpected child output'),
    redactor:{redactText:x=>x,addSensitiveValues(){}},finishTest:async result=>{events.push('finish');finishes.push(result);}};
  const ports={Date,resolve,e2eRoot:'/e2e',rangeExecutionServices:{},interruptedBy:null,activeChild:null,
    process:{env:{},platform:'controlled',stdout:{write(){}},stderr:{write(){}}},assertExistingDeviceRun(){},assertIosIdentityAdmission(){},
    createScenarioRuntimeProfile:()=>({kind:'ui'}),reportTestStage(){},updateReadinessReset(){},
    createIosIdentityContext:deny('iOS identity for no-iOS fixture'),prepareIosIdentityBoundary:deny('iOS fixture'),
    forwardScenarioProgress:deny('unexpected child progress'),readFileSync:path=>{events.push('readiness-read');assert.equal(path,'/controlled/readiness');return '{}';},
    assertScenarioReadiness:({outcome})=>{events.push('readiness-validate');assert.equal(outcome.exitCode,0);assert.equal(outcome.signal,null);},
    summarizeFailureFromLog:deny('success failure-summary')};
  const result=await actual('runEntry',ports)({testNumber:9,script:'test-9:block2',scenarios:[{path:'/original',value:{}}]},
    {attempt:1,runId:'new',collector,preserveEvidence:false,diagnosticScope:{kind:'diagnostic-block'}},
    {environmentGuard:{prepare:async()=>events.push('prepare')},batchProducts:{prepare:async()=>'/controlled/products',nativePreparation:()=>({})}},
    {prepareScenarioPlatforms:async()=>({platforms:[],reportPath:'/controlled/readiness'}),resetE2eScenarioState:async()=>({}),
      startPlatformLivenessMonitor:()=>{events.push('monitor-start');return()=>events.push('monitor-stop');},discoverAttemptArtifacts:()=>[],
      spawn:(command,argv,options)=>{
        events.push('spawn');environment=options.env;assert.equal(command,'npm');assert.deepEqual(plain(argv),['run','test-9:block2']);
        const stream={on:(name)=>{assert.equal(name,'data');}};
        const child={stdout:stream,stderr:stream,once:(name,callback)=>{
          assert(['error','close'].includes(name));
          if(name==='close')queueMicrotask(()=>{events.push('child-close-0');callback(0,null);});return child;
        }};return child;
      }});
  assert.equal(environment.E2E_PRESERVE_EVIDENCE,'1');assert.equal(environment.E2E_RANGE_RUN,'1');
  assert.equal(result.status,'passed','Raw scoped actor outcome remains separately qualified by caller');
  assert.equal(result.exitCode,0);assert.equal(result.signal,null);assert.equal(finishes.length,1);
  assert.equal(finishes[0].status,'diagnostic-completed');assert.equal(ports.activeChild,null);
  assert.deepEqual(events,['prepare','spawn','monitor-start','child-close-0','monitor-stop','readiness-read','readiness-validate','finish']);
});
