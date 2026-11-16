import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vm from 'node:vm';
import test from 'node:test';
import { withAndroidSdkPath } from './android-sdk.mjs';

function fixture(t, number, inherited = true) {
  const root = mkdtempSync(join(tmpdir(), 'metasecret-toolchain-path-'));
  const bins = Object.fromEntries(['jbr', 'cargo', 'brew', 'local', 'platform', 'emulator', 'override']
    .map(name => { const path = join(root, name); mkdirSync(path); return [name, path]; }));
  for (const [directory, file, output] of [
    ['jbr', 'java', 'inherited-jbr'], ['cargo', 'cargo', 'inherited-cargo'],
    ['brew', 'java', 'fallback-java'], ['brew', 'cargo', 'fallback-cargo'],
    ['override', 'java', 'explicit-java'],
  ]) writeFileSync(join(bins[directory], file), `#!/bin/sh\nprintf '%s' '${output}'\n`, { mode: 0o700 });
  const processes = []; const ownedChildClosures = new Map(); const calls = []; const quiet = [];
  const env = { JAVA_HOME: bins.jbr, CONTROLLED_PARENT_VALUE: 'retained' };
  if (inherited) env.PATH = `${bins.jbr}:${bins.cargo}:/usr/bin:/bin`;
  const fallbackBins = { '/opt/homebrew/bin': bins.brew, '/usr/local/bin': bins.local };
  const context = vm.createContext({
    process: { env, stdout: { write() {} }, stderr: { write() {} } },
    projectRoot: root, processes, ownedChildClosures, withAndroidSdkPath,
    androidSdkTools: { pathPrefix: `${bins.platform}:${bins.emulator}` },
    harnessShutdown: { assertCanSpawn() {} },
    appendRunLog: (stream, chunk) => quiet.push([stream, chunk.toString()]),
    spawn: (file, args, options) => {
      calls.push({ file, args, options });
      // Rehome only machine fallbacks. Actual wrapper ordering and OS executable resolution remain real.
      const childEnv = { ...options.env, PATH: options.env.PATH.split(':').map(value => fallbackBins[value] ?? value).join(':') };
      return spawn(file, args, { ...options, env: childEnv });
    },
  });
  const source = readFileSync(new URL(`../Test${number}/orchestrator.mjs`, import.meta.url), 'utf8');
  const start = source.indexOf('function run(command, args, options = {}) {');
  const end = source.indexOf('function runAndWait(', start);
  assert.ok(start >= 0 && end > start, 'Actual wrapper boundary missing');
  vm.runInContext(source.slice(start, end), context);
  t.after(async () => {
    // Every controlled child must naturally close before its temporary scripts are removed.
    for (const child of processes) {
      if (child.exitCode === null && child.signalCode === null) await once(child, 'close');
      assert.equal(child.exitCode, 0); assert.equal(child.signalCode, null);
    }
    rmSync(root, { recursive: true, force: true });
  });
  const invoke = async (file, options = {}) => {
    const child = context.run(file, [], { stdio: ['ignore', 'pipe', 'pipe'], captureQuiet: true, ...options });
    let output = ''; child.stdout.on('data', chunk => { output += chunk.toString(); });
    const [code, signal] = await once(child, 'close');
    assert.equal(code, 0); assert.equal(signal, null);
    assert.equal(processes.at(-1), child, 'Actual wrapper must retain its exact child');
    if (number !== 4) {
      assert.equal(ownedChildClosures.has(child), true, 'Actual small caller registers its exact close promise');
      await ownedChildClosures.get(child);
    }
    const call = calls.at(-1); assert.equal(call.options.shell, false);
    return { output, call };
  };
  return { bins, env, invoke, context, quiet };
}

for (const number of [1, 2, 3, 4]) {
  test(`actual Test${number} wrapper preserves inherited JBR/Cargo priority and SDK paths`, { timeout: 5000 }, async t => {
    const f = fixture(t, number);
    const java = await f.invoke('java');
    assert.equal(java.output, 'inherited-jbr', 'Inherited launcher must win over fallback Java');
    const cargo = await f.invoke('cargo'); assert.equal(cargo.output, 'inherited-cargo');
    const parts = java.call.options.env.PATH.split(':');
    assert.deepEqual(parts.slice(0, 2), [f.bins.platform, f.bins.emulator]);
    assert.ok(parts.indexOf(f.bins.jbr) < parts.indexOf('/opt/homebrew/bin'));
    assert.ok(parts.indexOf(f.bins.cargo) < parts.indexOf('/usr/local/bin'));
    assert.equal(java.call.options.env.JAVA_HOME, f.env.JAVA_HOME);
    assert.equal(java.call.options.env.CONTROLLED_PARENT_VALUE, 'retained');
    assert.equal(java.call.options.cwd, f.context.projectRoot);
  });

  test(`actual Test${number} wrapper missing or empty inherited PATH retains fallbacks without empty components`, { timeout: 5000 }, async t => {
    const f = fixture(t, number, false);
    for (const value of [undefined, '']) {
      if (value === undefined) delete f.env.PATH; else f.env.PATH = value;
      const result = await f.invoke('java'); assert.equal(result.output, 'fallback-java');
      assert.deepEqual(result.call.options.env.PATH.split(':'), [f.bins.platform, f.bins.emulator, '/opt/homebrew/bin', '/usr/local/bin']);
    }
  });

  test(`actual Test${number} wrapper explicit env.PATH/cwd overrides keep existing semantics`, { timeout: 5000 }, async t => {
    const f = fixture(t, number);
    const result = await f.invoke('java', { cwd: f.bins.override, env: { PATH: f.bins.override, CONTROLLED_PARENT_VALUE: 'override' } });
    assert.equal(result.output, 'explicit-java');
    assert.equal(result.call.options.env.PATH, f.bins.override);
    assert.equal(result.call.options.env.CONTROLLED_PARENT_VALUE, 'override');
    assert.equal(result.call.options.env.JAVA_HOME, f.env.JAVA_HOME);
    assert.equal(result.call.options.cwd, f.bins.override);
  });
}
