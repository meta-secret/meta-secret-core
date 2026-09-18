import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #7 uses the proven diagnostic lifecycle from Test #4 and enables its
// offline-receiver phase through the scenario configuration.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test7/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
