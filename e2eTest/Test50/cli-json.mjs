/** CLI diagnostics may precede pretty JSON; no failure ever echoes a wire payload. */
export function parseCliEnvelope(output) {
  const lines = output.split('\n');
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (!line.startsWith('{') && !/^\[\s*(?:$|\{|\])/.test(line)) continue;
    try { return JSON.parse(lines.slice(index).join('\n')); } catch { /* Try the next structured start. */ }
  }
  throw new Error('CLI did not return a complete JSON envelope (payload omitted)');
}
