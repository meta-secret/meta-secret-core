import assert from 'node:assert/strict';
import test from 'node:test';

import { createDiagnosticRedactor } from './diagnostic-redaction.mjs';

const redactor = createDiagnosticRedactor(['fixture secret with spaces']);

test('redacts exact configured Secret values', () => {
  const result = redactor.redactText('value=fixture secret with spaces');
  assert.equal(result, 'value=[REDACTED_SECRET]');
});

test('redacts quoted JSON key material variants', () => {
  const fixtures = {
    masterKey: 'camel-master',
    master_key: 'snake-master',
    privateKey: 'camel-private',
    private_key: 'snake-private',
    keyShare: 'camel-share',
    key_share: 'snake-share',
    secretValue: 'secret-value',
  };
  const result = redactor.redactText(JSON.stringify(fixtures));
  for (const secret of Object.values(fixtures)) assert.doesNotMatch(result, new RegExp(secret));
  assert.match(result, /"masterKey":"\[REDACTED\]"/);
  assert.match(result, /"private_key":"\[REDACTED\]"/);
  assert.match(result, /"keyShare":"\[REDACTED\]"/);
});

test('redacts scalar fields, watched arguments, and launch environment', () => {
  assert.equal(
    redactor.redactText('master-key=alpha, private key:beta; key_share=gamma'),
    'master-key=[REDACTED], private key:[REDACTED]; key_share=[REDACTED]',
  );
  assert.deepEqual(
    redactor.redactArguments([
      '-e', 'secretValue', 'argument-secret',
      '-e', 'private_key', 'private-argument-secret',
      '-e', 'master_key', 'master-argument-secret',
      '-e', 'keyShare', 'share-argument-secret',
      '--private-key', 'private-cli-secret',
      '--master_key', 'master-cli-secret',
      '--keyShare', 'share-cli-secret',
      '--master-key=inline-master-secret',
      '-Pandroid.testInstrumentationRunnerArguments.secretConfig={raw}',
    ]),
    [
      '-e', 'secretValue', '[REDACTED]',
      '-e', 'private_key', '[REDACTED]',
      '-e', 'master_key', '[REDACTED]',
      '-e', 'keyShare', '[REDACTED]',
      '--private-key', '[REDACTED]',
      '--master_key', '[REDACTED]',
      '--keyShare', '[REDACTED]',
      '--master-key=[REDACTED]',
      '-Pandroid.testInstrumentationRunnerArguments.secretConfig=[REDACTED]',
    ],
  );
  assert.deepEqual(
    redactor.redactEnvironment({
      E2E_SECRET_VALUE: 'environment-secret',
      PRIVATE_KEY: 'private-environment-secret',
      master_key: 'master-environment-secret',
      keyShare: 'share-environment-secret',
      SAFE_NAME: 'Secret1',
    }),
    {
      E2E_SECRET_VALUE: '[REDACTED]',
      PRIVATE_KEY: '[REDACTED]',
      master_key: '[REDACTED]',
      keyShare: '[REDACTED]',
      SAFE_NAME: 'Secret1',
    },
  );
});

test('redacts complete metacharacter-rich instrumentation values before serialization', () => {
  const metacharacterSecret = "before ' spaced $(not-run)\nafter";
  assert.deepEqual(
    redactor.redactArguments([
      '-e', 'secretValue', metacharacterSecret,
      '-e', 'privateKey', "private ' fragment",
      '-e', 'master_key', 'master $(fragment)',
      '-e', 'key_share', 'share,fragment',
    ]),
    [
      '-e', 'secretValue', '[REDACTED]',
      '-e', 'privateKey', '[REDACTED]',
      '-e', 'master_key', '[REDACTED]',
      '-e', 'key_share', '[REDACTED]',
    ],
  );
});

test('streaming redaction fails closed for a 600+ character JSON value split across chunks', () => {
  const stream = redactor.createStream({ maxBufferedRecordChars: 256 });
  const longPrivateKey = `begin-${'K'.repeat(700)}-end`;
  const chunks = [
    '{"pri',
    `vateKey":"${longPrivateKey.slice(0, 220)}`,
    longPrivateKey.slice(220, 520),
    `${longPrivateKey.slice(520)}"}\nvisible-safe-line\n`,
  ];
  const outputs = chunks.map((chunk) => stream.push(chunk));
  outputs.push(stream.flush());

  for (const output of outputs) {
    assert.doesNotMatch(output, /begin-|end|K{8}/);
  }
  const combined = outputs.join('');
  assert.match(combined, /\[REDACTED_STREAM_RECORD\]/);
  assert.match(combined, /visible-safe-line/);
});

test('streaming redaction retains split short fields until safely redacted', () => {
  const stream = redactor.createStream();
  const first = stream.push('{"master_');
  const second = stream.push('key":"split-master-secret"}\n');
  assert.equal(first, '');
  assert.doesNotMatch(second, /split-master-secret/);
  assert.match(second, /"master_key":"\[REDACTED\]"/);
});

test('streaming redaction quarantines multiline quoted PEM through its exact closure', () => {
  const stream = redactor.createStream({ maxBufferedRecordChars: 128 });
  const chunks = [
    '{"privateKey":"-----BEGIN PRIVATE KEY-----\n',
    'LINE1_PRIVATE_KEY_MATERIAL\n',
    'LINE2_SECRET_MATERIAL\n-----END PRI',
    'VATE KEY-----"}\nnormal-post-record-log\n',
  ];
  const outputs = chunks.map((chunk) => stream.push(chunk));
  outputs.push(stream.flush());

  for (const output of outputs) {
    assert.doesNotMatch(output, /LINE1_PRIVATE_KEY_MATERIAL/);
    assert.doesNotMatch(output, /LINE2_SECRET_MATERIAL/);
    assert.doesNotMatch(output, /END PRIVATE KEY/);
  }
  assert.doesNotMatch(outputs.slice(0, 3).join(''), /normal-post-record-log/);
  assert.match(outputs.join(''), /\[REDACTED_STREAM_RECORD\]/);
  assert.match(outputs.at(-2), /normal-post-record-log/);
});

test('streaming redaction never resynchronizes an uncertain PEM before stream close', () => {
  const stream = redactor.createStream({ maxBufferedRecordChars: 128 });
  const outputs = [
    stream.push('{"privateKey":"-----BEGIN PRIVATE KEY-----\n'),
    stream.push('LINE2_SECRET_MATERIAL\n"}\nnormal-must-remain-quarantined\n'),
    stream.flush(),
  ];

  for (const output of outputs) {
    assert.doesNotMatch(output, /LINE2_SECRET_MATERIAL/);
    assert.doesNotMatch(output, /normal-must-remain-quarantined/);
    assert.doesNotMatch(output, /BEGIN PRIVATE KEY/);
  }
  assert.match(outputs.join(''), /\[REDACTED_STREAM_RECORD\]/);
});
