import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { updateReadinessReport } from '../shared/platform-supervisor.mjs';

const coreRoot = resolve(fileURLToPath(new URL('../../meta-secret', import.meta.url)));
const serverUrl = 'http://127.0.0.1:3000';
const readinessParticipants = ['CLI/Core + local HTTP server', 'isolated local HTTP server'];
let serverProcess;
let serverTempDir;

function waitForExit(child, label) {
  return new Promise((resolvePromise, reject) => {
    child.once('error', (error) => reject(new Error(`${label} could not start: ${error.message}`)));
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolvePromise();
        return;
      }
      reject(new Error(`${label} failed: exit=${code ?? 'null'} signal=${signal ?? 'null'}`));
    });
  });
}

async function waitForHttp(url, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'no response';
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(2_000) });
      if (response.ok) return;
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
  }
  throw new Error(`Timed out waiting for ${url}: ${lastError}`);
}

async function portIsAlreadyUsed() {
  try {
    const response = await fetch(`${serverUrl}/hi`, { signal: AbortSignal.timeout(2_000) });
    return response.ok;
  } catch {
    return false;
  }
}

async function startLocalServer() {
  if (await portIsAlreadyUsed()) {
    throw new Error(
      `${serverUrl} is already in use. Stop the existing local meta-server before running Test #20 so the test gets an isolated database.`,
    );
  }

  // The standalone binary expects the production image's pre-migrated SQLite
  // file. Keep the test isolated while applying the same checked-in migration
  // before starting the real server process.
  serverTempDir = await mkdtemp(join(coreRoot, '.test20-server-'));
  const migrationPath = resolve(
    coreRoot,
    'db/sqlite/migrations/2023-04-22-065820_create_commit_log/up.sql',
  );
  const migrationSql = await readFile(migrationPath, 'utf8');
  const databasePath = join(serverTempDir, 'meta-secret.db');
  await new Promise((resolvePromise, reject) => {
    const migration = spawn('sqlite3', [databasePath], {
      stdio: ['pipe', 'inherit', 'inherit'],
    });
    migration.once('error', reject);
    migration.once('exit', (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`SQLite migration failed: exit=${code ?? 'null'} signal=${signal ?? 'null'}`));
    });
    migration.stdin.end(`${migrationSql}\n`);
  });
  const manifestPath = resolve(coreRoot, 'Cargo.toml');
  serverProcess = spawn(
    'cargo',
    ['run', '--quiet', '--manifest-path', manifestPath, '-p', 'meta-server'],
    {
      cwd: serverTempDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? 'info' },
    },
  );
  serverProcess.stdout.on('data', (chunk) => process.stdout.write(`[meta-server] ${chunk}`));
  serverProcess.stderr.on('data', (chunk) => process.stderr.write(`[meta-server] ${chunk}`));

  let onSpawnError;
  let onEarlyExit;
  const serverStartFailure = new Promise((_, reject) => {
    onSpawnError = (error) => reject(new Error(`Local meta-server could not start: ${error.message}`));
    onEarlyExit = (code, signal) => reject(new Error(
      `Local meta-server exited before /hi became healthy: exit=${code ?? 'null'} signal=${signal ?? 'null'}`,
    ));
    serverProcess.once('error', onSpawnError);
    serverProcess.once('exit', onEarlyExit);
  });
  try {
    await Promise.race([waitForHttp(`${serverUrl}/hi`), serverStartFailure]);
  } finally {
    serverProcess.removeListener('error', onSpawnError);
    serverProcess.removeListener('exit', onEarlyExit);
  }
  updateReadinessReport({
    platform: 'isolated local HTTP server',
    checks: { url: serverUrl, healthEndpoint: '/hi', healthy: true },
    requiredNonPlatformParticipants: readinessParticipants,
  });
  console.log(`✅ Local meta-server is ready at ${serverUrl}`);
}

function recordCliReadiness() {
  const checks = {};
  for (const [name, executable, args] of [
    ['cargo', 'cargo', ['--version']],
    ['sqlite3', 'sqlite3', ['--version']],
  ]) {
    const result = spawnSync(executable, args, { encoding: 'utf8' });
    if (result.error || result.status !== 0) {
      throw new Error(`Test #20 CLI readiness failed for ${name}: ${result.error?.message ?? result.stderr.trim()}`);
    }
    checks[name] = result.stdout.trim().split('\n')[0];
  }
  const coreManifest = resolve(coreRoot, 'Cargo.toml');
  const serverMigration = resolve(
    coreRoot,
    'db/sqlite/migrations/2023-04-22-065820_create_commit_log/up.sql',
  );
  if (!existsSync(coreManifest) || !existsSync(serverMigration)) {
    throw new Error('Test #20 CLI readiness failed: Core manifest or server migration is missing');
  }
  checks.coreManifest = coreManifest;
  checks.serverMigration = serverMigration;
  updateReadinessReport({
    platform: 'CLI/Core + local HTTP server',
    checks,
    requiredNonPlatformParticipants: readinessParticipants,
  });
  console.log('✅ Test #20 CLI/Core and local-server prerequisites verified before scenario checks');
}

async function stopLocalServer() {
  if (serverProcess && serverProcess.exitCode === null) {
    serverProcess.kill('SIGTERM');
    await new Promise((resolvePromise) => serverProcess.once('exit', resolvePromise));
  }
  serverProcess = undefined;
  if (serverTempDir) {
    await rm(serverTempDir, { recursive: true, force: true });
    serverTempDir = undefined;
  }
}

async function runCargoTest(args, label) {
  const child = spawn('cargo', args, { cwd: coreRoot, stdio: 'inherit' });
  await waitForExit(child, label);
  console.log(`✅ ${label} passed`);
}

async function main() {
  try {
    // Test20 declares no native UI apps: its participants are Rust Core, CLI
    // build tools, sqlite migration, and the isolated local HTTP server.
    if (await portIsAlreadyUsed()) {
      throw new Error(`${serverUrl} is already in use; cannot declare Test #20's isolated server ready`);
    }
    recordCliReadiness();
    // Keep the existing Core regression and add the real HTTP/server path.
    await runCargoTest(
      [
        'test',
        '-p',
        'meta-secret-core',
        'test20_accept_recover_ignores_foreign_and_unknown_claims',
        '--',
        '--nocapture',
      ],
      'Test #20 Core regression',
    );
    await startLocalServer();
    await runCargoTest(
      [
        'test',
        '-p',
        'meta-secret-tests',
        'test20_http_server_rejects_forged_recovery_responses',
        '--',
        '--nocapture',
      ],
      'Test #20 HTTP/server security path',
    );
    console.log('✅ Test #20 passed: forged recovery responses were rejected by the local server');
  } finally {
    await stopLocalServer();
  }
}

main().catch((error) => {
  console.error(`❌ Test #20 failed: ${error instanceof Error ? error.stack ?? error.message : error}`);
  process.exitCode = 1;
});
