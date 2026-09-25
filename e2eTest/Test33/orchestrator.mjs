import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #33 uses the existing Test #4 lifecycle, which already owns the local
// server, visible Web, Android, iOS, marker, screenshot, and cleanup helpers.
// The dedicated mode selects the iOS-owner branch without duplicating a second
// runner or changing any production behavior.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test33/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
