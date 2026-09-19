import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #11 reuses the shared lifecycle runner. Each block is a fresh
// Web-created vault and exercises a different sender platform.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test11/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
