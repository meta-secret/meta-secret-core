import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #15 reuses the stable Test #4 lifecycle harness and adds a CLI member
// during an active recovery claim.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test15/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
