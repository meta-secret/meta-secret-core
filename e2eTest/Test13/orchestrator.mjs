import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #13 checks that a repeated Approve click is harmless. The shared
// runner keeps the second click state-based, so the test does not introduce
// timing sleeps into the recovery flow.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test13/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
