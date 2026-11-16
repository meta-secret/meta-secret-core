import { invalid } from './coreValidation';

export class JsonNumber {
  constructor(readonly token: string) {}
}
type JsonValue = string | boolean | null | JsonNumber | JsonValue[] | { [key: string]: JsonValue };

/** Decode numeric tokens without ever constructing a rounded JavaScript Number. */
export function parseLosslessJson(source: string): JsonValue {
  if (typeof source !== 'string' || new TextEncoder().encode(source).length > 1_048_576) throw invalid();
  let index = 0;
  const whitespace = () => {
    while (/[\x20\t\r\n]/u.test(source[index] ?? 'x')) index++;
  };
  const string = (): string => {
    if (source[index] !== '"') throw invalid();
    const start = index++;
    while (index < source.length) {
      const char = source[index++];
      if (char === '\\') index++;
      else if (char === '"') return JSON.parse(source.slice(start, index));
    }
    throw invalid();
  };
  const value = (depth: number): JsonValue => {
    if (depth > 64) throw invalid();
    whitespace();
    if (source[index] === '"') return string();
    if (source[index] === '{') {
      index++;
      const object: { [key: string]: JsonValue } = Object.create(null);
      whitespace();
      if (source[index] === '}') {
        index++;
        return object;
      }
      while (index < source.length) {
        whitespace();
        const key = string();
        if (Object.hasOwn(object, key)) throw invalid();
        whitespace();
        if (source[index++] !== ':') throw invalid();
        object[key] = value(depth + 1);
        whitespace();
        const separator = source[index++];
        if (separator === '}') return object;
        if (separator !== ',') throw invalid();
      }
      throw invalid();
    }
    if (source[index] === '[') {
      index++;
      const items: JsonValue[] = [];
      whitespace();
      if (source[index] === ']') {
        index++;
        return items;
      }
      while (index < source.length) {
        items.push(value(depth + 1));
        whitespace();
        const separator = source[index++];
        if (separator === ']') return items;
        if (separator !== ',') throw invalid();
      }
      throw invalid();
    }
    for (const [literal, decoded] of [
      ['true', true],
      ['false', false],
      ['null', null],
    ] as const) {
      if (source.startsWith(literal, index)) {
        index += literal.length;
        return decoded;
      }
    }
    const token = source.slice(index).match(/^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/u)?.[0];
    if (!token) throw invalid();
    index += token.length;
    return new JsonNumber(token);
  };
  const result = value(0);
  whitespace();
  if (index !== source.length) throw invalid();
  return result;
}
