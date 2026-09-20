import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #4 contains the proven lifecycle/diagnostic runner. Test #5 supplies a
// different, explicitly claim-bound scenario while sharing the same runner.
// Keeping this adapter tiny prevents the two tests from drifting apart.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test5/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
