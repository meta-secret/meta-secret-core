import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #34 reuses the existing Web-secondary-creator lifecycle. It only
// selects a separate scenario and run ID; server, visible clients, markers,
// screenshots, and cleanup remain owned by Test #4.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test34/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
