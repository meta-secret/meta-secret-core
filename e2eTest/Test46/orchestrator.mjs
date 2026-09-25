import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #46 owns only its declarative removal/re-Join/recovery scenario.
// Test #4 retains local selection, one-time builds, visible clients,
// diagnostics, fail-together cancellation, cleanup, and exact restoration.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test46/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
