import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const coreRoot = resolve(fileURLToPath(new URL('../../meta-secret', import.meta.url)));
const child = spawn(
  'cargo',
  [
    'test',
    '-p',
    'meta-secret-core',
    'test20_accept_recover_ignores_foreign_and_unknown_claims',
    '--',
    '--nocapture',
  ],
  { cwd: coreRoot, stdio: 'inherit' },
);

child.once('error', (error) => {
  console.error(`Test #20 could not start cargo: ${error.message}`);
  process.exitCode = 1;
});
child.once('exit', (code, signal) => {
  if (code === 0) {
    console.log('✅ Test #20 invalid/foreign recovery response passed');
    return;
  }
  console.error(`❌ Test #20 failed: cargo exited with ${code ?? signal}`);
  process.exitCode = 1;
});
