import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { readFileSync, writeFileSync, mkdtempSync, rmSync, existsSync, mkdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { buildCreatePhaseFixture } from './ios-create-phase-fixture.mjs';
let root=resolve(import.meta.dirname);
while(!existsSync(join(root,'meta-secret-compose/iosApp/iosAppUITests/CaseTwoIosInitiatorUITest.swift'))){const p=dirname(root);assert.notEqual(p,root);root=p;}
const swift=join(root,'meta-secret-compose/iosApp/iosAppUITests');
let output,directory;
before(()=>{
 const sources=buildCreatePhaseFixture(readFileSync(process.env.CREATE_PHASE_CASE_TWO_PATH??join(swift,'CaseTwoIosInitiatorUITest.swift'),'utf8'),
  readFileSync(process.env.CREATE_PHASE_SECRET_PATH??join(swift,'LiveboardSecretTestActions.swift'),'utf8'),readFileSync(join(swift,'LiveboardTestActions.swift'),'utf8'));
 directory=process.env.CREATE_PHASE_OUTPUT_DIR??mkdtempSync(join(tmpdir(),'create-phase-bank-'));mkdirSync(directory,{recursive:true});
 for(const [name,body]of Object.entries(sources)){assert(body.split('\n').length<=400,name+' source budget');writeFileSync(join(directory,name+'.swift'),body);}
 const binary=join(directory,'phase-fixture');
 execFileSync('/usr/bin/xcrun',['swiftc',...['ports','owning','callers','main'].map(n=>join(directory,n+'.swift')),'-o',binary],{encoding:'utf8',timeout:60_000});
 output=JSON.parse(execFileSync(binary,[],{encoding:'utf8',timeout:10_000}));
});
after(()=>{if(directory&&!process.env.CREATE_PHASE_OUTPUT_DIR)rmSync(directory,{recursive:true,force:true});});
const flow=mode=>{const r=output.flows.find(x=>x.mode===mode);assert(r,mode);return r;};
function completeOriginalHappy(){
 const r=flow('plain');assert.equal(r.failures,0);
 assert.equal(r.opens,3);assert.equal(r.submits,3);assert.equal(r.fills,3);assert.equal(r.created,3);
 assert.equal(r.initialReady,true);assert.equal(r.secretsReady,true);assert.equal(r.approvals,2);return r;
}
test('actual CaseTwo complete original transcript precedes no perSubmit card probe',()=>{
 const r=completeOriginalHappy();assert.equal(r.initialScans,1);assert.equal(r.batches,2);
 if(r.initialSnapshots>0){assert.equal(r.initialSnapshots,3,'Old local happy probes completed');assert.equal(r.hidden,3);}
 assert.equal(r.initialSnapshots,0,'No perSubmit card observation before full-set phase barrier');
});
test('actual CaseTwo three modal-close proofs and two fresh exact all-name barriers preserve ordering',()=>{
 const r=completeOriginalHappy();assert.equal(r.singleScans,0);assert.equal(r.batches,2);assert.equal(r.fullScans,2);
 assert.deepEqual(r.batchSizes,[3,3]);assert.deepEqual(r.sameNames,[true,true]);assert.deepEqual(r.preBatchMappings,[0,3]);assert.equal(r.hidden,0);assert.equal(r.modalReads,6);
 assert.deepEqual(r.events,['OPEN_CREATE','SUBMIT','MODAL_COUNT','MODAL_COUNT','OPEN_CREATE','SUBMIT','MODAL_COUNT','MODAL_COUNT',
  'OPEN_CREATE','SUBMIT','MODAL_COUNT','MODAL_COUNT','FULL_BATCH','INITIAL_READY','JOIN','JOIN','POST_JOIN_ROUTE','FULL_BATCH','SECRETS_READY']);
});
test('actual modal refusal stops NEXT Create and all global READY and join actions',()=>{
 const r=flow('modal-present');assert(r.failures>0);assert.equal(r.opens,1);assert.equal(r.submits,1);assert.equal(r.created,1);
 assert.equal(r.initialReady,false);assert.equal(r.secretsReady,false);assert.equal(r.approvals,0);assert.equal(r.fullScans,0);
});
test('actual complete-set missing duplicate empty or unproven result refuses before READY and approvals',()=>{
 for(const mode of ['global-missing','global-duplicate','global-empty-id','global-unproven']){
  const r=flow(mode);assert(r.failures>0,mode);assert.equal(r.created,3,mode);assert.equal(r.batches,1,mode);
  assert.equal(r.initialReady,false,mode);assert.equal(r.secretsReady,false,mode);assert.equal(r.approvals,0,mode);
 }
});
test('actual postjoin fresh barrier cannot inherit initial successful set',()=>{
 const r=flow('postjoin-missing');assert(r.failures>0);assert.equal(r.batches,2);assert.equal(r.initialReady,true);
 assert.equal(r.approvals,2);assert.equal(r.secretsReady,false);
});
test('actual modal count positive and delayed close retain one shared30s phase before NEXT',()=>{
 for(const mode of ['plain','modal-delayed']){const r=flow(mode);assert.equal(r.failures,0,mode);
  assert.equal(r.created,3,mode);assert.equal(r.initialReady,true,mode);assert.equal(r.secretsReady,true,mode);
  assert.equal(r.initialSnapshots,0,mode);assert(r.modalReads>=6,mode);assert(r.modalCounts.every(n=>n===0||n===1),mode);
  assert(r.budgets.every(t=>t>0&&t<=30),mode);
 }
 assert.equal(flow('plain').budgets.length,0);assert.equal(flow('modal-delayed').budgets.length,3);
});
test('actual published duplicate offscreen reappeared or nonthrowing unresolved modal cannot authorize NEXT',()=>{
 for(const mode of ['modal-present','modal-duplicate','modal-offscreen','modal-reappears','modal-wait-return']){const r=flow(mode);
  assert(r.failures>0,mode);assert.equal(r.created,1,mode);assert.equal(r.opens,1,mode);assert.equal(r.submits,1,mode);
  assert.equal(r.batches,0,mode);assert.equal(r.initialReady,false,mode);assert.equal(r.secretsReady,false,mode);assert.equal(r.approvals,0,mode);
  assert(r.modalReads>=2,mode);assert(r.budgets.every(t=>t>0&&t<=30),mode);
 }
 assert.deepEqual(flow('modal-reappears').modalCounts,[0,1]);
 assert.deepEqual(flow('modal-wait-return').modalCounts,[1,1]);
});
test('actual late first or final modal getter cannot authorize NEXT or renew shared close deadline',()=>{
 for(const mode of ['modal-late-first','modal-late-final']){const r=flow(mode);assert(r.failures>0,mode);
  assert.equal(r.opens,1,mode);assert.equal(r.created,1,mode);assert.equal(r.batches,0,mode);
  assert.equal(r.initialReady,false,mode);assert.equal(r.secretsReady,false,mode);assert.equal(r.approvals,0,mode);
  assert.deepEqual(r.budgets,[],mode);assert.equal(r.modalReads,mode==='modal-late-first'?1:2,mode);
 }
});
