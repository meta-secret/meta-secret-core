/** Pure selection only; no module import may start a process, server, SDK or client. */
export function parseUiE2eRangeOptions(args) {
  const options = { from: 1, to: 50, runId: null, preserveEvidence: false, pauseAfterEachCase: false, help: false };
  const occurrences = new Map();
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    const key = argument.split('=', 1)[0];
    if (['--from', '--to', '--run-id'].includes(key)) occurrences.set(key, (occurrences.get(key) ?? 0) + 1);
    if (argument === '--help' || argument === '-h') options.help = true;
    else if (argument === '--preserve-evidence') options.preserveEvidence = true;
    else if (argument === '--pause-after-each-case') {
      if (options.pauseAfterEachCase) throw new Error('Duplicate --pause-after-each-case');
      options.pauseAfterEachCase = true;
    }
    else if (argument === '--diagnostic-block' || argument.startsWith('--diagnostic-block=')) {
      if (options.diagnosticBlock !== undefined) throw new Error('Duplicate --diagnostic-block');
      const value = argument.includes('=') ? argument.slice(argument.indexOf('=') + 1) : args[++index];
      if (!/^[1-9][0-9]*$/.test(value ?? '')) throw new Error('--diagnostic-block requires a positive canonical integer');
      options.diagnosticBlock = Number(value);
    } else if (['--from', '--to', '--run-id'].includes(argument)) {
      const value = args[++index];
      if (!value || value.startsWith('--')) throw new Error(`${argument} requires a value`);
      if (argument === '--from') options.from = Number(value);
      else if (argument === '--to') options.to = Number(value);
      else options.runId = value;
    } else if (argument.startsWith('--from=')) options.from = Number(argument.slice(7));
    else if (argument.startsWith('--to=')) options.to = Number(argument.slice(5));
    else if (argument.startsWith('--run-id=')) options.runId = argument.slice(9);
    else throw new Error(`Unknown option: ${argument}`);
  }
  if (options.diagnosticBlock !== undefined && [...occurrences.values()].some(count => count > 1)) throw new Error('Diagnostic identity/range options must not be duplicated');
  if (options.pauseAfterEachCase && options.diagnosticBlock !== undefined) throw new Error('--pause-after-each-case is only available for whole-case ranges');
  if (options.help) return options;
  if (!Number.isInteger(options.from) || !Number.isInteger(options.to) || options.from < 1 || options.to > 51 || options.from > options.to) throw new Error('Expected an inclusive test range within 1..51, with --from <= --to');
  if (options.runId && !/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(options.runId)) throw new Error('--run-id may contain only letters, digits, dot, underscore, and hyphen');
  if (options.diagnosticBlock !== undefined && (![9, 11].includes(options.from) || options.to !== options.from || options.diagnosticBlock !== 2 || !options.runId)) throw new Error('Diagnostic block requires exactly Test9/block2 or Test11/block2 and an explicit fresh --run-id');
  if (options.diagnosticBlock !== undefined) options.preserveEvidence = true;
  return options;
}

export function selectUiE2eCommand(number, scripts, orchestratorExists, diagnosticBlock) {
  if (diagnosticBlock !== undefined) {
    const key = `test-${number}:block2`;
    if (![9, 11].includes(number) || diagnosticBlock !== 2 || !orchestratorExists || scripts[key] !== `node Test${number}/orchestrator.mjs scenario-block2.json --exit-on-success`) throw new Error('Diagnostic block requires its exact allowlisted registered owning command');
    return key;
  }
  const script = scripts[`test-${number}:blocks`] ? `test-${number}:blocks` : scripts[`test-${number}`] ? `test-${number}` : null;
  if (!script) {
    if (number === 18) return null;
    throw new Error(`Test #${number} has no registered npm script`);
  }
  if (!orchestratorExists) throw new Error(`Test #${number} has a script but no orchestrator.mjs`);
  return script;
}
