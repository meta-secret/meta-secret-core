import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #37 owns only its declarative membership-removal scenario. The shared
// orchestrator remains responsible for the local environment guard, visible
// clients, diagnostics, screenshots, process cleanup, and exact restoration.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test37/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
