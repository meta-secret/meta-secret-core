// Permanent public-contract regressions with actual watcher/owner/registry bodies.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {EventEmitter} from 'node:events';
import {createRecoveryPhaseRegistry} from './recovery-phase.mjs';
import {createMatchedMarkerChoices} from './matched-marker-choice.mjs';
import {markerScan,markerScanStream,ssClaimsInvalidationMarker} from './process-marker.mjs';
import {assertRecoveryShowScope,runRecoveryIosShow} from './recovery-ios-show-phase.mjs';
const source=readFileSync(new URL('../Test4/orchestrator.mjs',import.meta.url),'utf8');
const ownerSource=readFileSync(new URL('./ios-selected-suite.mjs',import.meta.url),'utf8');
const profiles=[
  [26,'android-secondary-creator-lifecycle',[16,17,18,25,26,27,34,35,36]],
  [27,'ios-secondary-creator-lifecycle',[13,14,15,22,23,24,31,32,33]],
  [28,'web-secondary-creator-lifecycle',[7,8,9,16,17,18,25,26,27]],
  [29,'android-distributed-creator-lifecycle',[4,5,6,13,14,15,22,23,24]],
  [30,'ios-distributed-creator-lifecycle',[1,2,3,10,11,12,19,20,21]],
  [31,'web-secondary-creator-lifecycle',[7,8,9,16,17,18,25,26,27]],
  [32,'android-owner-creator-lifecycle',[4,5,6,13,14,15,22,23,24]],
  [33,'ios-two-secret-owner-lifecycle',[1,2,3,10,11,12,19,20,21]],
  [34,'web-secondary-creator-lifecycle',[5,6,11,12,17,18]],
  [35,'android-secondary-creator-lifecycle',[4,5,6,13,14,15,22,23,24]],
  [36,'ios-distributed-creator-lifecycle',[1,2,3,10,11,12,19,20,21]],
  [46,'web-distributed-creator-removal-rejoin-recovery-lifecycle',[25,26,27,28,29,30,31,32,33]],
  [47,'web-distributed-creator-declined-removal-recovery-lifecycle',[7,8,9,16,17,18,25,26,27]],
  [48,'android-distributed-creator-declined-removal-recovery-lifecycle',[7,8,9,16,17,18,25,26,27]],
  [49,'ios-distributed-creator-declined-removal-recovery-lifecycle',[7,8,9,16,17,18,25,26,27]],
];
function clockPort(){
  let time=0,sequence=0;const tasks=new Map();
  return{now:()=>time,set(fn,delay){assert(Number.isFinite(delay)&&delay>=0);const id=++sequence;tasks.set(id,{at:time+delay,fn});return id;},
    clear(id){tasks.delete(id);},jump(at){time=at;},
    async flush(){for(let i=0;i<1000;i++)await Promise.resolve();},
    async next(){if(!tasks.size)return false;time=Math.max(time,Math.min(...[...tasks.values()].map(value=>value.at)));
      for(const [id,value]of [...tasks])if(value.at<=time){tasks.delete(id);value.fn();}await this.flush();return true;},
    async drive(promise){let done=false,failure;promise.then(()=>done=true,error=>{done=true;failure=error;});await this.flush();
      for(let i=0;!done&&i<100;i++){if(!await this.next())break;}assert(done,'Synthetic actor stalled');return failure;},
  };
}
function fixture(cycle=16,{deadline=600_000,number=26}={}){
  const profile=profiles.find(row=>row[0]===number);assert(profile);
  const clock=clockPort(),child=new EventEmitter();child.stdout=new EventEmitter();child.stderr=new EventEmitter();
  child.exitCode=null;child.signalCode=null;
  child.kill=signal=>{child.signalCode=signal;child.emit('exit',null,signal);child.emit('close',null,signal);return true;};
  const begin=source.indexOf('function watchProcessOutput('),end=source.indexOf('\nfunction watchAndroidInstrumentation(',begin);
  assert(begin>=0&&end>begin);
  const context=vm.createContext({Buffer,Error,AggregateError,RegExp,Object,Date:{now:clock.now},performance:{now:clock.now},
    setTimeout:clock.set,clearTimeout:clock.clear,run:()=>child,createMatchedMarkerChoices,markerScan,markerScanStream,ssClaimsInvalidationMarker,
    scenario:{mode:profile[1]},diagnosticRedactor:{createStream:()=>({push:value=>value,flush:()=>''})},
    appendBounded:(old,next,limit=128000)=>(old+next).slice(-limit),maxWatchedOutputChars:128000,
    filterVerboseWatchedOutput:()=>'',process:{stdout:{write(){}},stderr:{write(){}}},appendFileSync(){},
    diagnosticLogPath:'PORT_NOT_WRITTEN',writeDiagnostic(){},redactCommandArguments:value=>value,redactEnvironment:value=>value,watchedProcessOutputs:[]});
  vm.runInContext(source.slice(begin,end)+'\nglobalThis.makeWatch=watchProcessOutput;'+ownerSource.replaceAll('export function ','function ')
    +'\nglobalThis.makeOwner=createIosSelectedSuiteActor;',context);
  const watched=context.makeWatch('closed-unit-port',[],{});void watched.result.catch(()=>{});
  const owner=context.makeOwner({watched,selector:'CaseEightIosBothOfflineUITest/handleRecoveryStep',timeoutMs:600_000});
  const registry=createRecoveryPhaseRegistry({selectedProfile:`test${number}:${profile[1]}`,now:clock.now,schedule:clock.set,cancel:clock.clear});
  const binding={version:1,runId:'SYNTHETIC_FINITE_SHOW',testNumber:number,attempt:1,blockId:'source-unit',cycle,logicalStep:1,
    mode:profile[1],contextId:'a'.repeat(32),sender:'ios',
    participants:['web','android','ios'].map((platform,index)=>({platform,role:platform==='ios'?'sender':'receiver',invocationId:String(index+1).repeat(32)}))};
  const phase=registry.register(binding),streams=Object.fromEntries(['android','ios'].map(platform=>[platform,phase.observer(platform,{})]));
  const event=(platform,name)=>{if(platform==='web'){phase.web(name);return;}const value=phase.contextFor(platform);
    const prefix=platform==='android'?'11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ':'';
    streams[platform].push('stdout',prefix+`E2E: RECOVERY_PHASE_V1 context=${value.contextId} invocation=${value.invocationId} platform=${platform} phase=${name}\n`);};
  for(const platform of ['web','android','ios'])event(platform,'READY');phase.releaseSender();event('ios','REQUEST_SENT');
  event('web','INCOMING_VISIBLE');event('android','INCOMING_VISIBLE');phase.requireBothBadges();phase.setIosDeadline(deadline);
  const expected=phase.contextFor('ios'),scope=Object.freeze({testNumber:number,role:'sender',nonce:'b'.repeat(32),cycle,step:1,
    runId:expected.runId,contextId:expected.contextId,invocationId:expected.invocationId});
  let joins=0,allowed=0;
  const waits=[],receipt=owner.selectedSuiteDeadline;
  const actor={...watched,result:owner.result,failure:owner.failure,recoveryShowPhase:scope,assertBusinessActive:owner.assertBusinessActive,
    get selectedSuiteDeadline(){return owner.selectedSuiteDeadline;},
    waitForMarker(marker,limit){waits.push({fresh:false,limit});return watched.waitForMarker(marker,limit);},
    waitForNextMarker(marker,limit){waits.push({fresh:true,limit});return watched.waitForNextMarker(marker,limit);},
    async cancelAndJoin(error){joins++;await owner.cancelAndJoin(error);}};
  const emit=(text,sink='stdout')=>child[sink].emit('data',Buffer.from(text));
  const prep=(s=scope,outcome='INPUT_RETURNED')=>`E2E: IOS_RECOVERY_SHOW_PREP_V1 ${s.testNumber} ${s.nonce} ${s.contextId} ${s.invocationId} ${s.cycle} ${s.step} ${outcome}\n`;
  const value=`E2E: IOS_RECOVERY_SECRET_VISIBLE_${cycle}_1\n`,close=`E2E: IOS_RECOVERY_CLOSED_${cycle}_1\n`;
  const initial=child.stdout.listenerCount('data')+child.stderr.listenerCount('data');
  return{clock,actor,phase,scope,expected,receipt,waits,child,emit,prep,value,close,
    get allowed(){return allowed;},get joins(){return joins;},
    listeners:()=>child.stdout.listenerCount('data')+child.stderr.listenerCount('data')-initial,
    run(schedule,{supplied=scope,now=clock.now,peerFailure}={}){return runRecoveryIosShow({actor,scope:supplied,phase,now,peerFailure,
      allowShow(){allowed++;assert(child.stdout.listenerCount('data')>1);schedule();}});},
    schedule(at,text,sink){clock.set(()=>emit(text,sink),at);},
    async stop(){registry.freeze(Error('SYNTHETIC_END'));if(child.exitCode==null&&child.signalCode==null)await owner.cancelAndJoin(Error('SYNTHETIC_END'));},
  };
}
async function success(f,outcome='INPUT_RETURNED'){
  const operation=f.run(()=>{f.schedule(44_175,f.prep(f.scope,outcome));f.schedule(48_050,f.value);f.schedule(49_050,f.close);});
  assert.ifError(await f.clock.drive(operation));assert.equal(f.allowed,1);assert.equal(f.listeners(),0);
  assert(f.waits.some(value=>value.limit===45_000));assert(f.waits.some(value=>value.limit===60_000));
}

test('all132 literal finite sender rows and both procedure outcomes succeed before scope negatives',async()=>{
  assert.equal(profiles.reduce((count,row)=>count+row[2].length,0),132);
  for(const[number,_mode,cycles]of profiles)for(const cycle of cycles)for(const outcome of ['INPUT_RETURNED','ALREADY_VISIBLE']){
    const f=fixture(cycle,{number});try{assert.equal(assertRecoveryShowScope(f.scope,f.expected),f.scope);await success(f,outcome);}
    finally{await f.stop();}
  }
  const f=fixture();try{
    for(const delta of [{testNumber:22},{role:'receiver'},{nonce:'b'.repeat(32)+'\n'},{cycle:18},{cycle:19.0+0.5},{step:2},
      {runId:'FOREIGN_RUN'},{contextId:'c'.repeat(32)},{invocationId:'d'.repeat(32)},{extra:true}]){
      assert.throws(()=>assertRecoveryShowScope(Object.freeze({...f.scope,...delta}),f.expected));
    }
    assert.throws(()=>assertRecoveryShowScope({...f.scope},f.expected));
    const hidden={...f.scope};Object.defineProperty(hidden,'hidden',{value:true});Object.freeze(hidden);
    assert.throws(()=>assertRecoveryShowScope(hidden,f.expected));
    const nonenumerable={...f.scope};Object.defineProperty(nonenumerable,'nonce',{value:f.scope.nonce,enumerable:false});Object.freeze(nonenumerable);
    assert.throws(()=>assertRecoveryShowScope(nonenumerable,f.expected),'An existing canonical field must serialize');
    let read=false;const getter={...f.scope};Object.defineProperty(getter,'nonce',{get(){read=true;return f.scope.nonce;}});Object.freeze(getter);
    assert.throws(()=>assertRecoveryShowScope(getter,f.expected));assert.equal(read,false,'Accessors are not scope data');
  }finally{await f.stop();}
});

for(const [name,packet]of[
  ['foreign nonce',f=>f.prep({...f.scope,nonce:'f'.repeat(32)})],
  ['foreign context',f=>f.prep({...f.scope,contextId:'c'.repeat(32)})],
  ['foreign invocation',f=>f.prep({...f.scope,invocationId:'d'.repeat(32)})],
  ['old22 marker',f=>`E2E: IOS_TEST22_SHOW_PREP_V1 ${f.scope.nonce} 19 1 INPUT_RETURNED\n`],
  ['passive diagnostic',()=> 'LB_SHOW_GATE_V1 stage=tap-complete count=UNKNOWN visible=UNKNOWN\n'],
  ['value before prep',f=>f.value+f.prep()+f.close],
  ['duplicate prep',f=>f.prep()+f.prep()+f.value+f.close],
  ['overlong prep',f=>f.prep().trimEnd()+'x'.repeat(513)+'\n'],
])test(name+' cannot grant finite scope value stage',async()=>{
  const f=fixture();try{const error=await f.clock.drive(f.run(()=>f.schedule(1,packet(f))));assert(error);assert.equal(f.joins,1);assert.equal(f.listeners(),0);}
  finally{await f.stop();}
});

test('current stream LF/CRLF split and ordered same-chunk events are accepted',async()=>{
  for(const sameChunk of [false,true]){const f=fixture();try{
    const text=f.prep().replace('\n','\r\n'),operation=f.run(()=>{
      if(sameChunk)f.schedule(1,text+f.value+f.close);
      else{f.schedule(1,text.slice(0,28));f.schedule(2,text.slice(28));f.schedule(3,f.value);f.schedule(4,f.close);}
    });assert.ifError(await f.clock.drive(operation));assert.equal(f.listeners(),0);
  }finally{await f.stop();}}
});

test('a current preparation line split before and after original release cannot authorize Show',async()=>{
  const valid=fixture();try{await success(valid);}finally{await valid.stop();}
  for(const split of [1,100]){const f=fixture();try{
    const packet=f.prep(),wait=f.actor.waitForNextMarker;
    f.actor.waitForNextMarker=(marker,limit)=>{const pending=wait.call(f.actor,marker,limit);f.emit(packet.slice(0,split));return pending;};
    const operation=f.run(()=>{f.schedule(1,packet.slice(split));f.schedule(2,f.value);f.schedule(3,f.close);});
    const error=await f.clock.drive(operation);assert.match(error?.message??'',/lacked a current ordered observation/);
    assert.equal(f.allowed,1);assert.equal(f.joins,1);assert.equal(f.listeners(),0);
  }finally{await f.stop();}}
});

test('pre-arm cached output, stderr and incomplete lines cannot authorize new Show',async()=>{
  for(const mode of ['cached','stderr','incomplete']){const f=fixture();try{
    if(mode==='cached')f.emit(f.prep());
    const operation=f.run(()=>{if(mode==='stderr')f.schedule(1,f.prep(),'stderr');
      if(mode==='incomplete')f.schedule(1,f.prep().trimEnd());f.schedule(2,f.value);f.schedule(3,f.close);});
    assert(await f.clock.drive(operation));assert.equal(f.joins,1);assert.equal(f.listeners(),0);
  }finally{await f.stop();}}
});

test('scope/owner receipt replacement and stdout end refuse without current completion',async()=>{
  for(const mode of ['scope','receipt','end']){const f=fixture();try{
    const operation=f.run(()=>f.clock.set(()=>{
      if(mode==='receipt')Object.defineProperty(f.actor,'selectedSuiteDeadline',{get:()=>({...f.receipt})});
      else if(mode==='scope')f.actor.recoveryShowPhase=Object.freeze({...f.scope});
      else f.child.stdout.emit('end');f.emit(f.prep()+f.value+f.close);
    },1));assert(await f.clock.drive(operation));assert.equal(f.joins,1);assert.equal(f.listeners(),0);
  }finally{await f.stop();}}
});

test('missing or nonordinary actor receipt refuses before original grant',async()=>{
  for(const receipt of [null,{timeoutMs:360_000,deadlineAt:600_000},{timeoutMs:600_000,deadlineAt:NaN}]){
    const f=fixture();try{Object.defineProperty(f.actor,'selectedSuiteDeadline',{get:()=>receipt});
      assert(await f.clock.drive(f.run(()=>{})));assert.equal(f.allowed,0);assert.equal(f.joins,1);assert.equal(f.listeners(),0);
    }finally{await f.stop();}
  }
});

test('invalid clock and already rejected preparation waiter prevent original release',async()=>{
  for(const mode of ['nan','backward','waiter']){const f=fixture();try{
    let reads=0;const primary=Error('SYNTHETIC_WAITER');
    if(mode==='waiter')f.actor.waitForNextMarker=()=>Promise.reject(primary);
    const operation=f.run(()=>{}, {now:mode==='nan'?()=>NaN:mode==='backward'?()=>reads++===0?100:0:f.clock.now});
    const error=await f.clock.drive(operation);assert(error);if(mode==='waiter')assert.equal(error,primary);
    assert.equal(f.allowed,0);assert.equal(f.joins,1);assert.equal(f.listeners(),0);
  }finally{await f.stop();}}
});

test('procedural receipt alone cannot replace exact value or close',async()=>{
  for(const mode of ['value','close']){const f=fixture();try{
    const operation=f.run(()=>f.schedule(1,f.prep()+(mode==='close'?f.value:'')));
    assert(await f.clock.drive(operation));assert.equal(f.joins,1);assert.equal(f.listeners(),0);
    assert(f.clock.now()<= (mode==='value'?45_001:60_001));
  }finally{await f.stop();}}
});

test('already failed current phase prevents release and retains that first cause',async()=>{
  const f=fixture(),primary=Error('SYNTHETIC_PHASE_PRIMARY');
  try{f.phase.abort(primary);const error=await f.clock.drive(f.run(()=>{}));
    assert.equal(error,primary);assert.equal(f.allowed,0);assert.equal(f.joins,1);assert.equal(f.listeners(),0);
  }finally{await f.stop();}
});

for(const [name,events,maximum]of[
  ['prep180',f=>[[180_001,f.prep()+f.value+f.close]],180_000],
  ['value45',f=>[[1,f.prep()],[45_002,f.value+f.close]],45_001],
  ['close60',f=>[[1,f.prep()],[2,f.value],[60_003,f.close]],60_002],
])test(name+' remains bounded without renewed whole timer',async()=>{
  const f=fixture();try{const error=await f.clock.drive(f.run(()=>{for(const [at,text]of events(f))f.schedule(at,text);}));
    assert(error);assert(f.clock.now()<=maximum);assert.equal(f.receipt.deadlineAt,600_000);assert.equal(f.joins,1);
  }finally{await f.stop();}
});

test('current D clips the same armed600 and late queued event cannot renew either',async()=>{
  const f=fixture(16,{deadline:5000});try{
    const operation=f.run(()=>f.schedule(20_000,f.prep()+f.value+f.close));
    assert(await f.clock.drive(operation));assert(f.clock.now()<=5000);assert.equal(f.receipt.deadlineAt,600_000);
  }finally{await f.stop();}
  const late=fixture();try{const operation=late.run(()=>{});await late.clock.flush();
    late.clock.jump(180_001);late.emit(late.prep()+late.value+late.close);
    assert(await late.clock.drive(operation));assert.equal(late.joins,1);
  }finally{await late.stop();}
});

test('scope object replacement, peer failure and cleanup failure preserve first cause and dispose',async()=>{
  const different=fixture();try{assert(await different.clock.drive(different.run(()=>{}, {supplied:Object.freeze({...different.scope})})));
    assert.equal(different.allowed,0);assert.equal(different.joins,1);
  }finally{await different.stop();}
  const f=fixture(),primary=Error('SYNTHETIC_PRIMARY'),cleanup=Error('SYNTHETIC_CLEANUP');let rejectPeer;
  const peerFailure=new Promise((_,reject)=>{rejectPeer=reject;});void peerFailure.catch(()=>{});
  f.actor.cancelAndJoin=async()=>{throw cleanup;};
  try{const operation=f.run(()=>f.clock.set(()=>rejectPeer(primary),1),{peerFailure});
    const error=await f.clock.drive(operation);assert(error instanceof AggregateError);
    assert.equal(error.errors[0],primary);assert.equal(error.errors[1],cleanup);assert.equal(f.listeners(),0);
  }finally{await f.stop();}
});
