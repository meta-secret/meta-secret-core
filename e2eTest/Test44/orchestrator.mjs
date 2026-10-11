import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #44 owns only its declarative distributed-creator removal scenario.
// Test #4 retains the local guard, one-time builds, visible clients,
// diagnostics, fail-together cancellation, cleanup, and exact restoration.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test44/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
