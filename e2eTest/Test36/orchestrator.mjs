import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #36 reuses the existing iOS distributed-creator lifecycle. Test #4
// remains the owner of the local server, visible clients, state-driven waits,
// marker timeline, screenshots, and cleanup.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test36/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
