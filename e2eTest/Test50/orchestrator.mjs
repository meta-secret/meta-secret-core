import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { buildDeletionPlan, validateDeletionPlan } from './plan.mjs';

const root = dirname(fileURLToPath(import.meta.url));
const runId = process.env.E2E_RUN_ID ?? `test50-${Date.now()}`;
const plan = buildDeletionPlan(runId);
console.log('📋 Test50 F1 matrix:', validateDeletionPlan(plan));
if (process.argv.includes('--validate-only')) process.exit(0);
if (process.env.E2E_RANGE_RUN !== '1') throw new Error('Run Test50 through npm run test-range -- --from 50 --to 50; its coordinator owns one local snapshot/restore');
const probeOnly = process.env.E2E_TEST50_ACCESSIBILITY_PROBE_ONLY === '1';
const probeRepetitions = Number(process.env.E2E_TEST50_ACCESSIBILITY_PROBE_REPETITIONS ?? '1');
if (!Number.isInteger(probeRepetitions) || probeRepetitions < 1 || probeRepetitions > 5 || (!probeOnly && probeRepetitions !== 1)) throw new Error('F3 probe repetitions require probe-only mode and integer1..5');
const firstAndroidBlock = plan.blocks.find((block) => block.members.includes('android'));
const diagnosticCycle = process.env.E2E_TEST50_DIAGNOSTIC_CYCLE;
if (diagnosticCycle !== undefined && (!['13', '16', '19', '23', '34'].includes(diagnosticCycle) || probeOnly)) throw new Error('Focused diagnostic supports only failed cycles13/16/19/23/34 and cannot combine with F3-only');
const diagnosticBlock = diagnosticCycle ? plan.blocks.find((block) => block.cycles.some((cycle) => cycle.number === Number(diagnosticCycle))) : null;
const selectedBlocks = diagnosticCycle ? [{ ...diagnosticBlock, runId: `${diagnosticBlock.runId}-diagnostic${diagnosticCycle}`, cycles: diagnosticBlock.cycles.filter((cycle) => diagnosticCycle === '34' ? cycle.number >= 25 && cycle.number <= 34 : diagnosticCycle === '23' ? cycle.number >= 19 && cycle.number <= 23 : cycle.number === Number(diagnosticCycle)) }] : probeOnly ? Array.from({ length: probeRepetitions }, (_, index) => firstAndroidBlock && ({ ...firstAndroidBlock, runId: `${firstAndroidBlock.runId}-f3-${index + 1}` })) : plan.blocks;
if (selectedBlocks.some((block) => !block)) throw new Error('Test50 Android F3 block is missing');
const base = JSON.parse(readFileSync(resolve(root, 'scenario.json'), 'utf8'));
const configDirectory = resolve(root, '..', '.artifacts', runId, 'test50-config');
mkdirSync(configDirectory, { recursive: true });
let owned;
const cancel = () => owned?.kill('SIGTERM');
process.once('SIGINT', cancel); process.once('SIGTERM', cancel);
for (const [index, block] of selectedBlocks.entries()) {
  const seed = { name: `${block.runId}-seed`, value: `${block.runId}-seed-value` };
  const scenario = { ...base, runId: block.runId, deletionBlock: block,
    vault: { name: `${block.runId}@test.ru` }, secret: seed,
    secrets: { seed, ...Object.fromEntries(block.cycles.map((cycle) => [cycle.secretName, { name: cycle.secretName, value: `${cycle.secretName}-value` }])) },
    secretCreation: { initial: { web: ['seed'] } },
    setup: { initiator: block.members[0] },
  };
  const path = resolve(configDirectory, `scenario-${block.id}.json`);
  writeFileSync(path, `${JSON.stringify(scenario, null, 2)}\n`);
  console.log(`📱 Test50 block ${index + 1}/${selectedBlocks.length}: ${block.id}; members=${block.members.join('/')}; cycles=${block.cycles.length}`);
  const result = await new Promise((resolveResult, reject) => {
    owned = spawn(process.execPath, [resolve(root, '..', 'Test4', 'orchestrator.mjs'), path, '--exit-on-success'], {
      cwd: resolve(root, '..'), stdio: 'inherit', env: { ...process.env,
        ...(index > 0 ? { E2E_REUSE_PROVEN_ARTIFACTS: '1' } : {}),
      },
    });
    owned.once('error', reject); owned.once('exit', (code, signal) => resolveResult({ code, signal }));
  });
  owned = null;
  if (result.code !== 0) throw new Error(`Test50 ${block.id} failed code=${result.code ?? result.signal}; preserve scoped run artifacts`);
}
if (probeOnly) {
  writeFileSync(resolve(configDirectory, 'f3-only-result.json'), `${JSON.stringify({ runId, stage: 'F3', accessibilityProbe: 'passed', probeRepetitions, F4: 'not-executed', businessAcceptance: false, businessCyclesExecuted: 0, declaredBusinessCycles: 60 }, null, 2)}\n`);
  console.log('✅ Test50 F3 accessibility-only proved; F4 not executed; 0/60 business cycles; no business acceptance');
} else if (diagnosticCycle) {
  writeFileSync(resolve(configDirectory, 'focused-diagnostic-result.json'), `${JSON.stringify({ runId, stage: 'F4-diagnostic', diagnosticCycle: Number(diagnosticCycle), businessAcceptance: false, businessCyclesExecuted: selectedBlocks[0].cycles.length, declaredBusinessCycles: 60 }, null, 2)}\n`);
  console.log(`✅ Test50 focused cycle${diagnosticCycle} diagnostic passed; ${selectedBlocks[0].cycles.length}/60 cycles; NOT full business acceptance`);
} else console.log('✅ Test50: all 8 blocks / 60 cycles complete');
