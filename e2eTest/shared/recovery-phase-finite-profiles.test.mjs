// Ordinary registry regressions. Synthetic scope/state checks, not SDK/UI results.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createRecoveryPhaseRegistry,createTest24RecoveryPhase} from './recovery-phase.mjs';
const profiles=[
  [26,'android-secondary-creator-lifecycle',13,39,27],[27,'ios-secondary-creator-lifecycle',13,39,27],
  [28,'web-secondary-creator-lifecycle',1,27,27],[29,'android-distributed-creator-lifecycle',1,27,27],
  [30,'ios-distributed-creator-lifecycle',1,27,27],[31,'web-secondary-creator-lifecycle',1,27,27],
  [32,'android-owner-creator-lifecycle',1,27,27],[33,'ios-two-secret-owner-lifecycle',1,27,27],
  [34,'web-secondary-creator-lifecycle',1,18,18],[35,'android-secondary-creator-lifecycle',1,27,27],
  [36,'ios-distributed-creator-lifecycle',1,27,27],
  [46,'web-distributed-creator-removal-rejoin-recovery-lifecycle',7,33,27],
  [47,'web-distributed-creator-declined-removal-recovery-lifecycle',1,27,27],
  [48,'android-distributed-creator-declined-removal-recovery-lifecycle',1,27,27],
  [49,'ios-distributed-creator-declined-removal-recovery-lifecycle',1,27,27],
];
const platforms=['web','android','ios'],clock={now:()=>0,schedule:()=>null,cancel(){}};
function binding(number,mode,cycle,sender='web',nonce='a'){
  return {version:1,runId:`SYNTHETIC_FINITE_${number}`,testNumber:number,attempt:1,blockId:'source-unit',cycle,
    logicalStep:1,mode,contextId:nonce.repeat(32),sender,participants:platforms.map((platform,index)=>({platform,
      role:platform===sender?'sender':'receiver',invocationId:String(index+1).repeat(32)}))};
}
function finish(phase,value){
  const streams=Object.fromEntries(['android','ios'].map(platform=>[platform,phase.observer(platform,{})]));
  const emit=(platform,event)=>{
    if(platform==='web'){phase.web(event);return;}
    const context=phase.contextFor(platform),prefix=platform==='android'?'11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ':'';
    streams[platform].push('stdout',prefix+`E2E: RECOVERY_PHASE_V1 context=${context.contextId} invocation=${context.invocationId} platform=${platform} phase=${event}\n`);
  };
  platforms.forEach(platform=>emit(platform,'READY'));phase.releaseSender();emit(value.sender,'REQUEST_SENT');
  platforms.filter(platform=>platform!==value.sender).forEach(platform=>emit(platform,'INCOMING_VISIBLE'));
  phase.requireBothBadges();phase.complete();
}
for(const [number,mode,first,last,cap]of profiles)test(`literal${number} positive boundaries and finite cap precede foreign25/default/refusal guards`,()=>{
  const selectedProfile=`test${number}:${mode}`;
  for(const cycle of [first,last])for(const sender of platforms){
    const registry=createRecoveryPhaseRegistry({...clock,selectedProfile}),value=binding(number,mode,cycle,sender),phase=registry.register(value);
    try{assert.equal(phase.contextFor('ios').testNumber,number);finish(phase,value);assert.throws(()=>registry.assertCycleActive(cycle));}
    finally{registry.freeze(Error('SYNTHETIC_END'));}
  }
  // Every real own profile has succeeded before its matching foreign25 guard.
  const foreign=binding(25,'web-secondary-creator-lifecycle',13),valid25=createRecoveryPhaseRegistry({...clock,selectedProfile:'test25:web-secondary-creator-lifecycle'});
  try{finish(valid25.register(foreign),foreign);}finally{valid25.freeze(Error('SYNTHETIC_END'));}
  for(const delta of [foreign,binding(number,mode,first-1),binding(number,mode,last+1),
    {...binding(number,mode,first),attempt:true},{...binding(number,mode,first),logicalStep:2}]){
    const registry=createRecoveryPhaseRegistry({...clock,selectedProfile});
    try{assert.throws(()=>registry.register(delta));}finally{registry.freeze(Error('SYNTHETIC_END'));}
  }
  assert.throws(()=>createTest24RecoveryPhase({...clock,binding:binding(number,mode,first)}));
  assert.throws(()=>createRecoveryPhaseRegistry(clock).register(binding(number,mode,first)));
  assert.equal(last-first+1,cap);
  const registry=createRecoveryPhaseRegistry({...clock,selectedProfile});
  try{
    for(let offset=0;offset<cap;offset++){
      const nonce=(offset+10).toString(16).padStart(32,'0'),value={...binding(number,mode,first+offset),contextId:nonce};
      finish(registry.register(value),value);
    }
    assert.throws(()=>registry.register(binding(number,mode,first)));
  }finally{registry.freeze(Error('SYNTHETIC_END'));}
});

test('finite catalog still refuses excluded, coerced and mixed number/mode selectors',()=>{
  for(const selectedProfile of ['test37:web-member-removal-lifecycle','test50:vault-secret-deletion',
    'test26:web-secondary-creator-lifecycle','test34:web-secondary-creator-lifecycle\n',true,null,26]){
    assert.throws(()=>createRecoveryPhaseRegistry({...clock,selectedProfile}));
  }
});
