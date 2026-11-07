export const invalid = () => new Error('OperationUnavailable');
export function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw invalid();
  return value as Record<string, unknown>;
}
export function text(value: unknown): string {
  if (typeof value !== 'string' || !value || new TextEncoder().encode(value).length > 4096) throw invalid();
  return value;
}
export function optionalText(value: unknown): string | undefined {
  return value === undefined ? undefined : text(value);
}
export function boolean(value: unknown): boolean {
  if (typeof value !== 'boolean') throw invalid();
  return value;
}
export function choice<T extends string>(value: unknown, values: readonly T[]): T {
  for (const option of values) if (value === option) return option;
  throw invalid();
}
export function array<T>(value: unknown, decode: (item: unknown) => T): T[] {
  if (!Array.isArray(value)) throw invalid();
  return value.map(decode);
}
export function unique(values: readonly string[]) {
  if (new Set(values).size !== values.length) throw invalid();
}
export function fields(value: Record<string, unknown>, permitted: readonly string[]) {
  if (Object.keys(value).some((key) => !permitted.includes(key))) throw invalid();
}
