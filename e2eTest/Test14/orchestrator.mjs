import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #14 submits the same sender recovery twice before the receiver answers.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test14/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
