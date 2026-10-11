import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Test #38 owns only the rotated Android/iOS/Web membership-removal scenario.
// Test #4 retains the local environment guard, visible-client lifecycle,
// diagnostics, screenshots, cancellation, cleanup, and exact restoration.
const forwardedArguments = process.argv.slice(2);
const scenarioArgument = forwardedArguments.find((argument) => !argument.startsWith('--'))
  ?? 'scenario.json';
const flags = forwardedArguments.filter((argument) => argument.startsWith('--'));
process.argv = [
  process.argv[0],
  resolve(fileURLToPath(new URL('../Test4/orchestrator.mjs', import.meta.url))),
  `../Test38/${scenarioArgument}`,
  ...flags,
];
await import('../Test4/orchestrator.mjs');
