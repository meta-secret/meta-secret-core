import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Reuse the diagnostic runner that drives Test #4 and Test #5. Test #6 only
// changes setup and claim data, so keeping one lifecycle implementation avoids
// introducing a second source of emulator and synchronization races.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test6/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
