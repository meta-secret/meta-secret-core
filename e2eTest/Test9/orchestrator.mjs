import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #9 uses the proven Test #4 lifecycle runner with a sender-offline
// recovery mode. Keeping this adapter tiny preserves the shared diagnostics
// and process cleanup used by the earlier tests.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario-block1.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test9/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
