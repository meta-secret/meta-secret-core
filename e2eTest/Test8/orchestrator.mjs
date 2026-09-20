import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #8 uses the shared lifecycle runner, but selects the dedicated
// Web-initiated / both-receivers-offline phase through its scenario.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test8/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
