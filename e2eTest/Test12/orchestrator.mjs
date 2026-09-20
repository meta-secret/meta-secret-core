import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #12 reuses the proven recovery lifecycle runner. Every receiver action
// is Decline; the next cycle creates a fresh recovery claim after the previous
// claim has reached its terminal declined state.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test12/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
