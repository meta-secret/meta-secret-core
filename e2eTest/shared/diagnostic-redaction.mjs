const sensitiveField = String.raw`(?:E2E_SECRET_VALUE|secret[_ -]?value|master[_ -]?key|private[_ -]?key|key[_ -]?share)`;
const quotedSensitiveFieldPattern = new RegExp(
  `((?:["']?${sensitiveField}["']?)\\s*[:=]\\s*)(["'])(.*?)\\2`,
  'gi',
);
const scalarSensitiveFieldPattern = new RegExp(
  `((?:["']?${sensitiveField}["']?)\\s*[:=]\\s*)(?!["'])([^\\r\\n,;}"']+)`,
  'gi',
);
const sensitiveAssignmentStartPattern = new RegExp(
  `(?:["']?${sensitiveField}["']?)\\s*[:=]`,
  'i',
);

function normalizedSensitiveKey(value) {
  return String(value ?? '')
    .replace(/^-+/, '')
    .replace(/[^a-z0-9]/gi, '')
    .toLowerCase();
}

function isSensitiveArgumentKey(value) {
  const normalized = normalizedSensitiveKey(value);
  return [
    'e2esecretvalue',
    'secretvalue',
    'secretvalues',
    'secretconfig',
    'masterkey',
    'privatekey',
    'keyshare',
  ].some((candidate) => normalized === candidate || normalized.endsWith(candidate));
}

export function createDiagnosticRedactor(rawSensitiveValues = []) {
  const sensitiveValues = [...new Set(
    rawSensitiveValues
      .filter((value) => typeof value === 'string' && value.length > 0),
  )].sort((left, right) => right.length - left.length);

  const redactText = (value) => {
    let redacted = String(value ?? '');
    for (const sensitiveValue of sensitiveValues) {
      redacted = redacted.split(sensitiveValue).join('[REDACTED_SECRET]');
    }
    return redacted
      .replace(quotedSensitiveFieldPattern, '$1$2[REDACTED]$2')
      .replace(scalarSensitiveFieldPattern, '$1[REDACTED]');
  };

  const redactArguments = (args) => args.map((argument, index) => {
    const previous = index > 0 ? String(args[index - 1]) : '';
    if (isSensitiveArgumentKey(previous)) return '[REDACTED]';
    const argumentText = String(argument);
    const equalsIndex = argumentText.indexOf('=');
    if (equalsIndex >= 0 && isSensitiveArgumentKey(argumentText.slice(0, equalsIndex))) {
      return `${argumentText.slice(0, equalsIndex + 1)}[REDACTED]`;
    }
    return redactText(argument);
  });

  const redactEnvironment = (environment = {}) => Object.fromEntries(
    Object.entries(environment).map(([key, value]) => [
      key,
      /SECRET_(?:VALUE|CONFIG)|MASTER[_ -]?KEY|PRIVATE[_ -]?KEY|KEY[_ -]?SHARE/i.test(key)
        ? '[REDACTED]'
        : redactText(value),
    ]),
  );

  const createStream = ({ maxBufferedRecordChars = 512 } = {}) => {
    if (!Number.isInteger(maxBufferedRecordChars) || maxBufferedRecordChars < 64) {
      throw new Error('maxBufferedRecordChars must be an integer >= 64');
    }
    let pending = '';
    let sensitiveQuarantine = null;
    let discardLongRecordUntilNewline = false;
    const failClosedReplacement = '[REDACTED_STREAM_RECORD]';
    const pemBeginPattern = /-----BEGIN ((?:[A-Z0-9]+[ -]+)*PRIVATE KEY)-----/;
    const maxPemMarkerChars = 256;

    const jsonContainerDepth = (prefix) => {
      let depth = 0;
      let quote = null;
      let escaped = false;
      for (const character of prefix) {
        if (quote) {
          if (escaped) escaped = false;
          else if (character === '\\') escaped = true;
          else if (character === quote) quote = null;
          continue;
        }
        if (character === '"' || character === "'") quote = character;
        else if (character === '{' || character === '[') depth += 1;
        else if (character === '}' || character === ']') depth = Math.max(0, depth - 1);
      }
      return depth;
    };

    const updatePemState = (state, character) => {
      state.pemMarkerTail = `${state.pemMarkerTail}${character}`.slice(-maxPemMarkerChars);
      if (!state.pemEndMarker) {
        const beginMatch = state.pemMarkerTail.match(pemBeginPattern);
        if (beginMatch) state.pemEndMarker = `-----END ${beginMatch[1]}-----`;
      }
      if (state.pemEndMarker && state.pemMarkerTail.endsWith(state.pemEndMarker)) {
        state.pemEndSeen = true;
      }
    };

    const createSensitiveQuarantine = (record, assignmentMatch) => ({
      phase: 'value-start',
      quote: null,
      escaped: false,
      jsonDepth: jsonContainerDepth(record.slice(0, assignmentMatch.index)),
      jsonQuote: null,
      jsonEscaped: false,
      pemMarkerTail: '',
      pemEndMarker: null,
      pemEndSeen: false,
    });

    const consumeSensitiveQuarantine = (state, value) => {
      let index = 0;
      while (index < value.length) {
        const character = value[index];

        if (state.phase === 'value-start') {
          if (/\s/.test(character)) {
            index += 1;
            continue;
          }
          if (character === '"' || character === "'") {
            state.phase = 'quoted';
            state.quote = character;
            index += 1;
            continue;
          }
          state.phase = 'unquoted';
          continue;
        }

        if (state.phase === 'quoted') {
          updatePemState(state, character);
          if (state.escaped) state.escaped = false;
          else if (character === '\\') state.escaped = true;
          else if (character === state.quote) {
            // A PEM-looking value may not finish at an earlier quote: without
            // its exact END marker the syntactic closure is unproven, so the
            // rest of the stream remains quarantined through close().
            state.phase = state.pemEndMarker && !state.pemEndSeen
              ? 'uncertain'
              : 'after-value';
          }
          index += 1;
          continue;
        }

        if (state.phase === 'unquoted') {
          updatePemState(state, character);
          if (state.pemEndMarker) {
            state.phase = 'pem';
            index += 1;
            continue;
          }
          if (character === ',' || character === ';' || character === '}') {
            state.phase = 'after-value';
            if (character === '}' && state.jsonDepth > 0) state.jsonDepth -= 1;
            index += 1;
            continue;
          }
          if (character === '\n') {
            index += 1;
            if (state.jsonDepth === 0) return { consumed: index, closed: true };
            state.phase = 'after-value';
            continue;
          }
          index += 1;
          continue;
        }

        if (state.phase === 'pem') {
          updatePemState(state, character);
          if (state.pemEndSeen) state.phase = 'after-value';
          index += 1;
          continue;
        }

        if (state.phase === 'after-value') {
          if (state.jsonQuote) {
            if (state.jsonEscaped) state.jsonEscaped = false;
            else if (character === '\\') state.jsonEscaped = true;
            else if (character === state.jsonQuote) state.jsonQuote = null;
          } else if (character === '"' || character === "'") {
            state.jsonQuote = character;
          } else if (character === '{' || character === '[') {
            state.jsonDepth += 1;
          } else if ((character === '}' || character === ']') && state.jsonDepth > 0) {
            state.jsonDepth -= 1;
          }
          index += 1;
          if (character === '\n' && state.jsonDepth === 0 && !state.jsonQuote) {
            return { consumed: index, closed: true };
          }
          continue;
        }

        // `uncertain` deliberately has no resynchronization point. A malformed
        // quoted/PEM field cannot make later bytes safe; close() drops them.
        index = value.length;
      }
      return { consumed: index, closed: false };
    };

    const beginSensitiveQuarantine = (record, assignmentMatch) => {
      const state = createSensitiveQuarantine(record, assignmentMatch);
      const valueStart = assignmentMatch.index + assignmentMatch[0].length;
      const result = consumeSensitiveQuarantine(state, record.slice(valueStart));
      return result.closed ? null : state;
    };

    const sanitizeCompleteRecord = (record) => {
      const safe = redactText(record);
      const assignmentMatch = sensitiveAssignmentStartPattern.exec(record);
      if (assignmentMatch && safe === record) {
        return {
          text: `${failClosedReplacement}${record.endsWith('\n') ? '\n' : ''}`,
          quarantine: beginSensitiveQuarantine(record, assignmentMatch),
        };
      }
      return { text: safe, quarantine: null };
    };

    const push = (value) => {
      pending += String(value ?? '');
      let output = '';
      while (pending.length > 0) {
        if (sensitiveQuarantine) {
          const result = consumeSensitiveQuarantine(sensitiveQuarantine, pending);
          pending = pending.slice(result.consumed);
          if (!result.closed) {
            pending = '';
            break;
          }
          sensitiveQuarantine = null;
          output += '\n';
          continue;
        }

        if (discardLongRecordUntilNewline) {
          const newlineIndex = pending.indexOf('\n');
          if (newlineIndex < 0) {
            pending = '';
            break;
          }
          pending = pending.slice(newlineIndex + 1);
          output += '\n';
          discardLongRecordUntilNewline = false;
          continue;
        }

        const newlineIndex = pending.indexOf('\n');
        if (newlineIndex >= 0) {
          const record = pending.slice(0, newlineIndex + 1);
          pending = pending.slice(newlineIndex + 1);
          const sanitized = sanitizeCompleteRecord(record);
          output += sanitized.text;
          sensitiveQuarantine = sanitized.quarantine;
          continue;
        }

        if (pending.length > maxBufferedRecordChars) {
          const assignmentMatch = sensitiveAssignmentStartPattern.exec(pending);
          output += failClosedReplacement;
          if (assignmentMatch) {
            sensitiveQuarantine = beginSensitiveQuarantine(pending, assignmentMatch);
          } else {
            // No field grammar was found before the bounded buffer filled.
            // Drop this opaque record through its next newline.
            discardLongRecordUntilNewline = true;
          }
          pending = '';
        }
        break;
      }
      return output;
    };

    const flush = () => {
      if (sensitiveQuarantine || discardLongRecordUntilNewline) {
        pending = '';
        sensitiveQuarantine = null;
        discardLongRecordUntilNewline = false;
        return '';
      }
      if (!pending) return '';
      const sanitized = sanitizeCompleteRecord(pending);
      pending = '';
      return sanitized.text;
    };

    return { push, flush };
  };

  return {
    redactText,
    redactArguments,
    redactEnvironment,
    createStream,
  };
}
