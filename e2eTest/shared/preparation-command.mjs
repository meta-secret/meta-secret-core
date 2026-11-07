/** Complete output is finite; diagnostic tails never substitute for capture. */
export async function capturePreparationCommand(command, args, options, lifecycle) {
  const { cwd, env, onOutput, allowFailure = false, timeoutMs = 30000, maxCaptureBytes = 64 * 1024 * 1024 } = options;
  if (!Number.isSafeInteger(maxCaptureBytes) || maxCaptureBytes < 1) throw new Error('Preparation capture requires a finite positive byte bound');
  if (lifecycle.interrupted()) throw new Error(`Preparation interrupted by ${lifecycle.interrupted()}`);
  const child = lifecycle.spawn(command, args, { cwd, env, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
  child.detached = process.platform !== 'win32'; child.preparation = true;
  lifecycle.started(child);
  const streams = { stdout: { chunks: [], bytes: 0, tail: Buffer.alloc(0) }, stderr: { chunks: [], bytes: 0, tail: Buffer.alloc(0) } };
  let overflow = false; let timedOut = false; let spawnError; let totalBytes = 0;
  for (const [name, stream] of Object.entries(streams)) child[name].on('data', (value) => {
    const chunk = Buffer.from(value); stream.bytes += chunk.length; totalBytes += chunk.length;
    stream.tail = Buffer.concat([stream.tail, chunk]).subarray(-65536);
    onOutput?.(name, chunk);
    if (!overflow && totalBytes > maxCaptureBytes) {
      overflow = true; streams.stdout.chunks = []; streams.stderr.chunks = [];
      lifecycle.signal(child, 'SIGTERM');
    }
    if (!overflow) stream.chunks.push(chunk);
  });
  const timer = setTimeout(() => { timedOut = true; lifecycle.signal(child, 'SIGTERM'); }, timeoutMs);
  let result;
  try {
    result = await new Promise((resolveResult) => {
      child.once('error', (error) => { spawnError = error; });
      // The close barrier drains the owned child even after error/overflow.
      child.once('close', (exitCode, signal) => resolveResult({ exitCode, signal }));
    });
  } finally { clearTimeout(timer); lifecycle.settled(child); }
  const diagnostics = Object.fromEntries(Object.entries(streams).map(([name, stream]) => [name, { bytes: stream.bytes, tailBytes: stream.tail.length }]));
  if (overflow || timedOut || lifecycle.interrupted() || spawnError || (result.exitCode !== 0 && !allowFailure)) {
    const reason = overflow ? 'capture overflow' : timedOut ? 'timeout' : lifecycle.interrupted() ?? (spawnError ? 'spawn error' : result.exitCode ?? result.signal);
    const error = new Error(`Preparation command ${command} failed (${reason}); raw output omitted`);
    error.code = overflow ? 'PREPARATION_CAPTURE_OVERFLOW' : 'PREPARATION_COMMAND_FAILED'; error.captureDiagnostics = diagnostics;
    throw error;
  }
  return { ...result, stdout: Buffer.concat(streams.stdout.chunks).toString('utf8'), stderr: Buffer.concat(streams.stderr.chunks).toString('utf8'),
    diagnosticTails: { stdout: streams.stdout.tail.toString('utf8'), stderr: streams.stderr.tail.toString('utf8') } };
}
