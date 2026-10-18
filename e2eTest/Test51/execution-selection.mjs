/** Select execution order only; the canonical plan and its case objects stay intact. */
export function selectRegistrationExecution(cases, args) {
  const options = new Map();
  for (const argument of args) {
    if (argument === '--validate-only') continue;
    const match = /^(--case|--cases|--first-case|--from-case)=(.+)$/.exec(argument);
    if (!match) throw new Error('Test51 accepts --validate-only, --case=<variant>, --cases=<variant>,<variant>, --first-case=<variant> or --from-case=<variant>');
    if (options.has(match[1])) throw new Error(`Repeated registration selector: ${match[1]}`);
    options.set(match[1], match[2]);
  }
  if (options.size > 1) throw new Error('--case, --cases, --first-case and --from-case cannot be combined');
  const selectedIds = options.has('--cases') ? options.get('--cases').split(',') : [...options.values()];
  if (selectedIds.some((id) => !id)) throw new Error('Registration case list must not contain empty IDs');
  if (new Set(selectedIds).size !== selectedIds.length) throw new Error('Registration case list must not contain duplicate IDs');
  for (const id of selectedIds) {
    if (!cases.some((entry) => entry.id === id)) throw new Error(`Unknown registration case: ${id}`);
  }
  if (options.has('--cases')) return selectedIds.map((id) => cases.find((entry) => entry.id === id));
  const selected = options.get('--case');
  if (selected) return cases.filter(({ id }) => id === selected);
  const from = options.get('--from-case');
  if (from) return cases.slice(cases.findIndex(({ id }) => id === from));
  const first = options.get('--first-case');
  return first ? [cases.find(({ id }) => id === first), ...cases.filter(({ id }) => id !== first)] : [...cases];
}
