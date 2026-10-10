import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { existsSync, lstatSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, relative, resolve, sep } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { reclaimBuildCaches } from './shared/build-cache-cleanup.mjs';

const e2eRoot = resolve(fileURLToPath(new URL('.', import.meta.url)));
const coreRoot = resolve(e2eRoot, '..');
const workspaceRoot = resolve(coreRoot, '..');
const home = homedir();
const lowSpaceThresholdGiB = 15;
const requiredPostCleanupGiB = 12;
const results = [];

function freeSpaceGiB() {
  const result = spawnSync('df', ['-Pk', e2eRoot], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`df failed: ${result.stderr || result.error?.message}`);
  const fields = result.stdout.trim().split(/\r?\n/).at(-1).trim().split(/\s+/);
  const availableKiB = Number(fields[3]);
  if (!Number.isFinite(availableKiB)) throw new Error('Could not parse available disk space');
  return Number((availableKiB / 1024 / 1024).toFixed(2));
}

function run(command, args, { cwd, allowFailure = false } = {}) {
  try {
    const output = execFileSync(command, args, {
      cwd,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      maxBuffer: 16 * 1024 * 1024,
    });
    return { ok: true, output };
  } catch (error) {
    const message = `${error.stderr?.toString() ?? ''}${error.stdout?.toString() ?? ''}${error.message}`.trim();
    if (allowFailure) return { ok: false, output: message };
    throw new Error(`${command} ${args.join(' ')} failed: ${message}`);
  }
}

function isInsideHome(target) {
  const path = resolve(target);
  return path === home || path.startsWith(`${home}${sep}`);
}

function removeExact(target) {
  const path = resolve(target);
  if (!isInsideHome(path)) throw new Error(`Refusing to remove path outside home: ${path}`);
  if (!existsSync(path)) return false;
  const info = lstatSync(path);
  if (info.isSymbolicLink()) throw new Error(`Refusing to follow/remove symlink: ${path}`);
  rmSync(path, { recursive: true, force: true });
  results.push(`removed ${path}`);
  return true;
}

function appIsRunning(names) {
  const { output } = run('ps', ['-axo', 'command=']);
  const commands = output.split(/\r?\n/).map((line) => line.toLowerCase());
  return names.some((name) => commands.some((command) => command.includes(name.toLowerCase())));
}

function cleanApplicationCache(name, paths, processNames) {
  if (appIsRunning(processNames)) {
    results.push(`skipped ${name}: app is running`);
    return;
  }
  for (const path of paths) removeExact(join(home, path));
}

function cleanOldUserLogs() {
  const logRoot = join(home, 'Library/Logs');
  if (!existsSync(logRoot)) return;
  // find does not follow symlinks by default; only regular files older than 30 days are eligible.
  const result = run('find', [logRoot, '-type', 'f', '-mtime', '+30', '-delete'], { allowFailure: true });
  if (!result.ok) results.push(`warning: could not remove old user logs: ${result.output}`);
  else results.push(`removed regular files older than 30 days under ${logRoot}`);
}

function cleanXcodeBuildData() {
  if (appIsRunning(['Xcode', 'xcodebuild'])) {
    results.push('skipped Xcode DerivedData and Archives: Xcode or xcodebuild is running');
    return;
  }
  removeExact(join(home, 'Library/Developer/Xcode/DerivedData'));
  removeExact(join(home, 'Library/Developer/Xcode/Archives'));
}

function cleanConda() {
  const conda = join(home, 'miniconda3/bin/conda');
  const executable = existsSync(conda) ? conda : 'conda';
  const result = run(executable, ['clean', '--all', '--yes'], { allowFailure: true });
  results.push(result.ok ? 'ran conda clean --all --yes' : `warning: conda cleanup unavailable: ${result.output}`);
}

function cleanGradle() {
  const wrapper = join(workspaceRoot, 'meta-secret-compose/gradlew');
  if (existsSync(wrapper)) {
    const stopped = run(wrapper, ['--stop'], { cwd: join(workspaceRoot, 'meta-secret-compose'), allowFailure: true });
    if (!stopped.ok) {
      results.push(`warning: Gradle stop failed: ${stopped.output}`);
      const processList = run('ps', ['-axo', 'command='], { allowFailure: true });
      if (!processList.ok || /GradleDaemon/.test(processList.output)) {
        results.push('skipped ~/.gradle/caches: Gradle daemons may still be running');
        return;
      }
    } else {
      results.push('stopped Gradle daemons using the Compose wrapper');
    }
  }
  const processList = run('ps', ['-axo', 'command='], { allowFailure: true });
  if (!processList.ok || /GradleDaemon/.test(processList.output)) {
    results.push('skipped ~/.gradle/caches: Gradle daemons may still be running');
    return;
  }
  removeExact(join(home, '.gradle/caches'));
}

function cleanExpo() {
  for (const name of ['expo-go', 'ios-simulator-app-cache', 'native-modules-cache', 'schema-cache', 'versions-cache']) {
    removeExact(join(home, '.expo', name));
  }
}

function cleanHomebrew() {
  const dryRun = run('brew', ['cleanup', '--dry-run'], { allowFailure: true });
  if (!dryRun.ok) {
    results.push(`warning: brew cleanup --dry-run failed: ${dryRun.output}`);
    return;
  }
  const candidates = /(?:Would remove|Would uninstall|Would autoremove|Removing|Uninstalling)\s/i.test(dryRun.output);
  if (candidates) {
    results.push(`brew cleanup skipped; review dry-run candidates:\n${dryRun.output.trim()}`);
    return;
  }
  const cleanup = run('brew', ['cleanup'], { allowFailure: true });
  results.push(cleanup.ok ? 'ran brew cleanup after an empty dry-run' : `warning: brew cleanup failed: ${cleanup.output}`);
}

const scanExclusions = new Set(['.git', 'node_modules', 'target', 'Pods', 'DerivedData', 'build', '.gradle']);
const projectRoots = [workspaceRoot, join(home, 'Documents/Projects'), join(home, 'Projects')]
  .filter((path, index, all) => existsSync(path) && all.indexOf(path) === index);

function scanProjectFiles(directory, accept, depth = 0) {
  if (depth > 9) return [];
  let entries;
  try { entries = readdirSync(directory, { withFileTypes: true }); } catch { return []; }
  const found = [];
  for (const entry of entries) {
    if (entry.isDirectory() && !scanExclusions.has(entry.name)) {
      found.push(...scanProjectFiles(join(directory, entry.name), accept, depth + 1));
    } else if (entry.isFile() && accept(entry.name)) {
      found.push(join(directory, entry.name));
    }
  }
  return found;
}

function pinnedRustChannels() {
  const pins = new Set();
  for (const path of projectRoots.flatMap((root) => scanProjectFiles(root, (name) => name === 'rust-toolchain' || name === 'rust-toolchain.toml'))) {
    const content = readFileSync(path, 'utf8');
    const channel = content.match(/^\s*channel\s*=\s*["']([^"']+)/m)?.[1] ?? content.trim();
    if (channel) pins.add(channel);
  }
  return pins;
}

function numericVersion(value) {
  const match = value.match(/(?:^|[^0-9])(\d+)\.(\d+)(?:\.(\d+))?(?:$|[^0-9])/);
  return match ? [Number(match[1]), Number(match[2]), Number(match[3] ?? 0)] : null;
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function highestNumericVersion(names) {
  return names.map((name) => numericVersion(name)).filter(Boolean)
    .sort((left, right) => compareVersions(right, left))[0] ?? null;
}

function pruneRustupToolchains() {
  const toolchainRoot = join(home, '.rustup/toolchains');
  const listed = run('rustup', ['toolchain', 'list'], { allowFailure: true });
  if (!listed.ok || !existsSync(toolchainRoot)) {
    results.push(`skipped Rust toolchain pruning: ${listed.output || 'rustup/toolchains unavailable'}`);
    return;
  }
  const commands = run('ps', ['-axo', 'command='], { allowFailure: true });
  if (!commands.ok || /(?:^|\s)(?:rustc|cargo)(?:\s|$)/.test(commands.output)) {
    results.push('skipped Rust toolchain pruning: cargo or rustc is running');
    return;
  }
  const names = listed.output.split(/\r?\n/).map((line) => line.trim().split(/\s+/)[0]).filter(Boolean);
  const active = listed.output.match(/^([^\s]+)\s+\(active(?:, default)?\)/m)?.[1]
    ?? listed.output.match(/^([^\s]+)\s+\(default(?:, active)?\)/m)?.[1];
  const pins = pinnedRustChannels();
  const newestVersion = highestNumericVersion(names);
  const removable = names.filter((name) => name !== active
    && !(newestVersion && numericVersion(name) && compareVersions(numericVersion(name), newestVersion) === 0)
    && ![...pins].some((pin) => name === pin || name.startsWith(`${pin}-`)));
  for (const name of removable) {
    const result = run('rustup', ['toolchain', 'uninstall', name], { allowFailure: true });
    results.push(result.ok ? `removed unpinned Rust toolchain ${name}` : `warning: Rust toolchain ${name} was not removed: ${result.output}`);
  }
  if (removable.length === 0) results.push('Rust toolchains retained: active, newest, and all project-pinned versions');
}

function pinnedPythonVersions() {
  const pins = new Set();
  const accepted = (name) => ['.python-version', '.tool-versions', 'runtime.txt', 'pyproject.toml', 'environment.yml', 'environment.yaml'].includes(name);
  for (const path of projectRoots.flatMap((root) => scanProjectFiles(root, accepted))) {
    const content = readFileSync(path, 'utf8');
    if (path.endsWith('.python-version')) {
      for (const line of content.split(/\r?\n/)) if (/^\s*\d+\.\d+(?:\.\d+)?\s*$/.test(line)) pins.add(line.trim());
    }
    for (const match of content.matchAll(/(?:python(?:-version)?|python_version)\s*(?:=|:|\s)\s*["']?(\d+\.\d+(?:\.\d+)?)/gi)) pins.add(match[1]);
    for (const match of content.matchAll(/^\s*python\s+(\d+\.\d+(?:\.\d+)?)/gim)) pins.add(match[1]);
    for (const match of content.matchAll(/requires-python\s*=\s*["'][^0-9]*(\d+\.\d+(?:\.\d+)?)/gi)) pins.add(match[1]);
  }
  return pins;
}

function prunePyenvVersions() {
  const versionsRoot = join(home, '.pyenv/versions');
  if (!existsSync(versionsRoot)) return;
  const commands = run('ps', ['-axo', 'command='], { allowFailure: true });
  if (!commands.ok || commands.output.includes(join(home, '.pyenv/versions/'))) {
    results.push('skipped pyenv version pruning: a pyenv interpreter may be running');
    return;
  }
  const names = readdirSync(versionsRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name);
  const versioned = names.filter((name) => numericVersion(name));
  const newestVersion = highestNumericVersion(versioned);
  const pins = pinnedPythonVersions();
  for (const name of versioned) {
    if ((newestVersion && compareVersions(numericVersion(name), newestVersion) === 0)
      || [...pins].some((pin) => name === pin || name.startsWith(`${pin}-`))) continue;
    const result = run('pyenv', ['uninstall', '--force', name], { allowFailure: true });
    results.push(result.ok ? `removed unpinned pyenv Python ${name}` : `warning: pyenv Python ${name} was not removed: ${result.output}`);
  }
  if (versioned.length <= 1) results.push('pyenv retained its only installed Python version');
}

function pinnedKotlinVersions() {
  const pins = new Set();
  for (const path of projectRoots.flatMap((root) => scanProjectFiles(root, (name) => name === 'libs.versions.toml' || name === 'build.gradle' || name === 'build.gradle.kts'))) {
    const content = readFileSync(path, 'utf8');
    for (const match of content.matchAll(/^\s*kotlin\s*=\s*["'](\d+\.\d+(?:\.\d+)?)["']/gm)) pins.add(match[1]);
    for (const match of content.matchAll(/org\.jetbrains\.kotlin[^\n]*version\s*["'](\d+\.\d+(?:\.\d+)?)["']/g)) pins.add(match[1]);
  }
  return pins;
}

function pruneKonanVersions() {
  const konanRoot = join(home, '.konan');
  if (!existsSync(konanRoot)) return;
  const commands = run('ps', ['-axo', 'command='], { allowFailure: true });
  if (!commands.ok || /GradleDaemon|KotlinCompileDaemon/.test(commands.output)) {
    results.push('skipped Kotlin/Native cache pruning: Gradle or Kotlin compilation is running');
    return;
  }
  const entries = readdirSync(konanRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && /^kotlin-native-prebuilt-/.test(entry.name));
  const names = entries.map((entry) => entry.name);
  const newestVersion = highestNumericVersion(names);
  const pins = pinnedKotlinVersions();
  for (const entry of entries) {
    const version = numericVersion(entry.name)?.join('.');
    if ((newestVersion && numericVersion(entry.name) && compareVersions(numericVersion(entry.name), newestVersion) === 0)
      || (version && [...pins].some((pin) => version === pin))) continue;
    removeExact(join(konanRoot, entry.name));
  }
  if (entries.length <= 1) results.push('Kotlin/Native retained its only prebuilt bundle');
  // ~/.konan/dependencies is intentionally never touched.
}

async function resetRequestedAndroidAvd(avdName) {
  if (avdName !== 'Pixel_4a') {
    if (avdName) results.push(`skipped Android userdata reset: ${avdName} is not the approved Pixel_4a AVD`);
    return;
  }
  const sdk = process.env.ANDROID_SDK_ROOT || process.env.ANDROID_HOME || join(home, 'Library/Android/sdk');
  const adb = join(sdk, 'platform-tools/adb');
  const emulator = join(sdk, 'emulator/emulator');
  if (!existsSync(adb) || !existsSync(emulator)) {
    results.push('skipped Pixel_4a userdata reset: Android SDK tools are unavailable');
    return;
  }
  const processList = run('ps', ['-axo', 'command='], { allowFailure: true });
  if (processList.ok && processList.output.split(/\r?\n/).some((line) => /(?:^|\s)-avd\s+Pixel_4a(?:\s|$)/.test(line))) {
    results.push('skipped Pixel_4a userdata reset: emulator process for this AVD already exists');
    return;
  }
  const devices = run(adb, ['devices'], { allowFailure: true });
  if (!devices.ok) {
    results.push(`skipped Pixel_4a userdata reset: adb devices failed: ${devices.output}`);
    return;
  }
  for (const line of devices.output.split(/\r?\n/)) {
    const serial = line.trim().split(/\s+/)[0];
    if (!serial?.startsWith('emulator-')) continue;
    const avd = run(adb, ['-s', serial, 'emu', 'avd', 'name'], { allowFailure: true });
    if (avd.ok && avd.output.includes('Pixel_4a')) {
      results.push(`skipped Pixel_4a userdata reset: AVD is already online (${serial})`);
      return;
    }
  }
  const userdataImage = join(home, '.android/avd/Pixel_4a.avd/userdata-qemu.img.qcow2');
  const qemuCandidates = [
    join(sdk, 'emulator/qemu/darwin-aarch64/qemu-img'),
    join(sdk, 'emulator/qemu/darwin-x86_64/qemu-img'),
    'qemu-img',
  ];
  const qemuImg = qemuCandidates.find((candidate) => candidate === 'qemu-img' || existsSync(candidate));
  if (!qemuImg || !existsSync(userdataImage) || lstatSync(userdataImage).isSymbolicLink()) {
    results.push('skipped Pixel_4a userdata reset: writable userdata image or qemu-img could not be safely verified');
    return;
  }
  const info = run(qemuImg, ['info', userdataImage], { allowFailure: true });
  if (!info.ok) {
    results.push(`skipped Pixel_4a userdata reset: qemu-img info failed: ${info.output}`);
    return;
  }
  results.push(`inspected Pixel_4a writable userdata image with qemu-img info: ${info.output.trim().replace(/\r?\n/g, '; ')}`);
  const launched = spawn(emulator, ['-avd', 'Pixel_4a', '-wipe-data'], { detached: true, stdio: 'ignore' });
  launched.unref();
  const launch = await new Promise((resolvePromise) => {
    launched.once('spawn', () => resolvePromise({ ok: true }));
    launched.once('error', (error) => resolvePromise({ ok: false, error }));
  });
  results.push(launch.ok
    ? 'started the approved visible Pixel_4a AVD with -wipe-data'
    : `warning: Pixel_4a wipe-data launch failed: ${launch.error.message}`);
}

async function automaticCleanup(avdName, force = false) {
  const beforeGiB = freeSpaceGiB();
  process.stdout.write(`💽 Free space before cleanup: ${beforeGiB} GiB\n`);
  if (!force && beforeGiB >= lowSpaceThresholdGiB) {
    process.stdout.write('🧹 Cleanup skipped: free space is above the 15 GiB threshold.\n🧹 Очистил 0 ГБ\n');
    return { beforeGiB, afterGiB: beforeGiB, cleanedGiB: 0, skipped: true };
  }

  const npmClean = run('npm', ['cache', 'clean', '--force'], { allowFailure: true });
  results.push(npmClean.ok ? 'ran npm cache clean --force' : `warning: npm cache cleanup failed: ${npmClean.output}`);
  cleanConda();
  cleanOldUserLogs();
  cleanApplicationCache('Brave', [
    'Library/Caches/BraveSoftware/Brave-Browser/Default/Cache',
    'Library/Caches/BraveSoftware/Brave-Browser/Default/Code Cache',
  ], ['Brave Browser']);
  cleanApplicationCache('Cursor', ['Library/Application Support/Cursor/CachedData'], ['Cursor']);
  cleanApplicationCache('Claude', [
    'Library/Application Support/Claude/Cache',
    'Library/Application Support/Claude/Code Cache',
  ], ['Claude']);
  cleanApplicationCache('Linear', [
    'Library/Application Support/Linear/Cache',
    'Library/Application Support/Linear/Code Cache',
  ], ['Linear']);
  removeExact(join(home, 'java_error_in_studio.hprof'));
  removeExact(join(home, 'fvm'));
  removeExact(join(home, '.pub-cache'));
  cleanExpo();
  pruneRustupToolchains();
  prunePyenvVersions();
  pruneKonanVersions();
  cleanHomebrew();
  await resetRequestedAndroidAvd(avdName);

  reclaimBuildCaches({
    measureFreeGiB: freeSpaceGiB,
    minimumGiB: requiredPostCleanupGiB,
    buildersIdle: () => {
      const processes = run('ps', ['-axo', 'command='], { allowFailure: true });
      return processes.ok && !/(?:^|\s)(?:\S*\/)?(?:cargo|rustc|xcodebuild)(?:\s|$)|GradleWrapperMain|GradleWorkerMain|KotlinCompileDaemon/m.test(processes.output);
    },
    cleanups: [cleanXcodeBuildData, cleanGradle, () => {
      const cargo = run('cargo', ['clean', '--manifest-path', join(coreRoot, 'meta-secret/Cargo.toml')], { cwd: coreRoot, allowFailure: true });
      results.push(cargo.ok ? 'ran cargo clean as last low-space fallback for the meta-secret-core workspace' : `warning: cargo clean failed: ${cargo.output}`);
    }],
    report: (message) => results.push(message),
  });

  const afterGiB = freeSpaceGiB();
  const cleanedGiB = Math.max(0, Number((afterGiB - beforeGiB).toFixed(2)));
  process.stdout.write(`💽 Free space after cleanup: ${afterGiB} GiB\n`);
  for (const result of results) process.stdout.write(`• ${result}\n`);
  process.stdout.write(`🧹 Очистил ${cleanedGiB} ГБ\n`);
  return { beforeGiB, afterGiB, cleanedGiB, skipped: false, belowMinimum: afterGiB < requiredPostCleanupGiB };
}

function pruneSuccessfulRun(runId) {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(runId)) throw new Error('Invalid run id');
  const runDirectory = resolve(e2eRoot, '.artifacts/runs', runId);
  const manifestPath = join(runDirectory, 'run.json');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (manifest.status !== 'completed') throw new Error(`Refusing cleanup: run status is ${manifest.status}`);
  const passedTests = new Set(manifest.tests.filter((test) => test.status === 'passed').map((test) => test.test));
  const expectedTests = [];
  for (let test = manifest.range.from; test <= manifest.range.to; test += 1) {
    if (test === 18 && manifest.skippedTests?.some((entry) => entry.test === 18)) continue;
    expectedTests.push(test);
  }
  if (expectedTests.some((test) => !passedTests.has(test))) throw new Error('Refusing cleanup: not every executable test has a passing attempt');
  const testLogs = join(runDirectory, 'tests');
  if (existsSync(testLogs)) {
    const directoryInfo = lstatSync(testLogs);
    if (directoryInfo.isSymbolicLink() || !directoryInfo.isDirectory()) {
      throw new Error(`Refusing cleanup: unexpected per-test log directory type ${testLogs}`);
    }
  }
  const beforeGiB = freeSpaceGiB();
  let bytesRemoved = 0;
  for (const attempt of manifest.tests) {
    if (attempt.status !== 'passed' || attempt.rawLogRetained === false || !attempt.logPath) continue;
    const logPath = resolve(attempt.logPath);
    if (resolve(logPath, '..') !== resolve(testLogs) || !/^test-\d{2}-attempt-\d+\.log$/.test(logPath.split(sep).at(-1))) {
      throw new Error(`Refusing cleanup: manifest log path is outside its per-test log folder: ${logPath}`);
    }
    if (existsSync(logPath)) {
      const info = lstatSync(logPath);
      if (info.isSymbolicLink() || !info.isFile()) throw new Error(`Refusing cleanup: unexpected log file type ${logPath}`);
      bytesRemoved += info.size;
      rmSync(logPath);
    }
  }
  const artifactRoot = resolve(e2eRoot, '.artifacts');
  const allowedArtifactName = /^test-(\d+)-(?:run-)?([A-Za-z0-9._-]+)-attempt-(\d+)-.+\.(?:xcresult|log|json|txt|png)$/i;
  for (const attempt of manifest.tests) {
    if (attempt.status !== 'passed' || attempt.ownedArtifactsRetained === false) continue;
    for (const registeredPath of attempt.ownedArtifacts ?? []) {
      const artifactPath = resolve(registeredPath);
      const name = artifactPath.split(sep).at(-1);
      const match = name.match(allowedArtifactName);
      const ownedPrefixes = [
        `test-${attempt.test}-run-${runId}-attempt-${attempt.attempt}-`,
        `test-${attempt.test}-${runId}-attempt-${attempt.attempt}-`,
      ];
      if (resolve(artifactPath, '..') !== artifactRoot
          || !match
          || Number(match[1]) !== attempt.test
          || Number(match[3]) !== attempt.attempt
          || !ownedPrefixes.some((prefix) => name.startsWith(prefix))) {
        throw new Error(`Refusing cleanup: artifact is not an exact registered passing-attempt artifact: ${artifactPath}`);
      }
      if (!existsSync(artifactPath)) continue;
      const info = lstatSync(artifactPath);
      if (info.isSymbolicLink() || (!info.isFile() && !info.isDirectory())) {
        throw new Error(`Refusing cleanup: unexpected artifact type ${artifactPath}`);
      }
      bytesRemoved += info.isFile() ? info.size : 0;
      rmSync(artifactPath, { recursive: info.isDirectory(), force: false });
      attempt.ownedArtifactsRetained = false;
    }
  }
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
  // Only remove the now-empty directory; failed attempts remain for analysis.
  if (existsSync(testLogs) && readdirSync(testLogs).length === 0) rmSync(testLogs);
  const afterGiB = freeSpaceGiB();
  const measuredByFilesGiB = Number((bytesRemoved / (1024 ** 3)).toFixed(4));
  process.stdout.write(`🧹 Successful run ${runId}: removed passing-attempt logs and registered passing-attempt artifacts; failed-attempt evidence, readiness reports, run.json, and run.log preserved.\n`);
  process.stdout.write(`💽 Available space ${beforeGiB} → ${afterGiB} GiB\n🧹 Очистил ${measuredByFilesGiB} ГБ\n`);
  return { beforeGiB, afterGiB, cleanedGiB: measuredByFilesGiB };
}

async function main(args) {
  if (args[0] === '--prune-success-run' && args[1]) {
    pruneSuccessfulRun(args[1]);
    return 0;
  }
  const avdArgument = args.find((arg) => arg.startsWith('--avd='));
  const report = await automaticCleanup(avdArgument?.slice('--avd='.length), args.includes('--force'));
  return report.belowMinimum ? 1 : 0;
}

main(process.argv.slice(2)).then((code) => {
  process.exitCode = code;
}).catch((error) => {
  process.stderr.write(`Disk cleaner stopped safely: ${error.stack ?? error.message}\n`);
  process.exitCode = 1;
});
