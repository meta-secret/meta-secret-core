import type { CheckedReceipt, ProducerReceipt, ProducerSource } from './coreTypes';
import { choice, fields, invalid, optionalText, record, text } from './coreValidation';
import { JsonNumber, parseLosslessJson } from './losslessJson';

const uuid = (value: unknown) => {
  const id = text(value);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u.test(id)) throw invalid();
  return id;
};
const phase = (value: unknown) => choice(value, ['queued', 'unknown']);
const observation = (value: unknown) =>
  choice(value, ['unconfirmed', 'persisted', 'admitted', 'rawAcknowledged', 'absent', 'redacted', 'conflict']);
const u64 = (value: unknown): bigint => {
  if (!(value instanceof JsonNumber) || !/^(0|[1-9]\d*)$/u.test(value.token)) throw invalid();
  const number = BigInt(value.token);
  if (number > 18_446_744_073_709_551_615n) throw invalid();
  return number;
};

export function parseOperationReceipt(raw: string): CheckedReceipt {
  try {
    const dto = record(parseLosslessJson(raw));
    fields(dto, ['kind', 'secretId', 'claimId', 'requestId', 'phase', 'errorCode']);
    const kind = choice(dto.kind, ['create', 'recover', 'join', 'removeDevice']);
    const secretId = optionalText(dto.secretId),
      claimId = optionalText(dto.claimId);
    const requestId = dto.requestId === undefined ? undefined : uuid(dto.requestId);
    if (
      (kind === 'create' && !secretId) ||
      (kind === 'recover' && (!secretId || !claimId)) ||
      ((kind === 'join' || kind === 'removeDevice') && !requestId)
    )
      throw invalid();
    return { kind, secretId, claimId, requestId, phase: phase(dto.phase), errorCode: optionalText(dto.errorCode) };
  } catch {
    throw invalid();
  }
}
function source(value: unknown): ProducerSource {
  const dto = record(value);
  fields(dto, ['eventId', 'bodyCommitment']);
  const event = record(dto.eventId);
  fields(event, ['fqdn', 'id']);
  const fqdn = record(event.fqdn),
    id = record(event.id);
  fields(fqdn, ['objType', 'objInstance']);
  fields(id, ['curr', 'prev']);
  const curr = u64(id.curr),
    prev = u64(id.prev),
    bodyCommitment = text(dto.bodyCommitment);
  if (curr === 0n || prev !== curr - 1n || !/^[0-9a-f]{64}$/u.test(bodyCommitment)) throw invalid();
  return {
    eventId: { fqdn: { objType: text(fqdn.objType), objInstance: text(fqdn.objInstance) }, id: { curr, prev } },
    bodyCommitment,
  };
}
function attempt(value: unknown) {
  const dto = record(value);
  fields(dto, ['source', 'observation']);
  return { source: source(dto.source), observation: observation(dto.observation) };
}
export function parseProducerReceipt(raw: string): ProducerReceipt {
  try {
    const dto = record(parseLosslessJson(raw));
    fields(dto, [
      'version',
      'kind',
      'phase',
      'secretId',
      'targetDeviceId',
      'requestId',
      'requestStage',
      'decisionStage',
      'materialStage',
      'canonicalState',
      'readiness',
      'errorCode',
    ]);
    if (!(dto.version instanceof JsonNumber) || dto.version.token !== '1') throw invalid();
    const kind = choice(dto.kind, ['deleteSecret', 'removeDevice']);
    const secretId = optionalText(dto.secretId),
      targetDeviceId = optionalText(dto.targetDeviceId);
    const requestId = dto.requestId === undefined ? undefined : uuid(dto.requestId);
    if (
      (kind === 'deleteSecret' && (!secretId || !requestId || dto.decisionStage !== undefined)) ||
      (kind === 'removeDevice' && !targetDeviceId)
    )
      throw invalid();
    let decisionStage: ProducerReceipt['decisionStage'];
    if (dto.decisionStage !== undefined) {
      const decision = record(dto.decisionStage);
      const state = choice(decision.state, ['notAttempted', 'attempted']);
      fields(decision, state === 'notAttempted' ? ['state'] : ['state', 'source', 'observation']);
      decisionStage =
        state === 'notAttempted'
          ? { state }
          : {
              state,
              source: source(decision.source),
              observation: observation(decision.observation),
            };
    }
    return {
      version: 1,
      kind,
      phase: phase(dto.phase),
      secretId,
      targetDeviceId,
      requestId,
      requestStage: attempt(dto.requestStage),
      decisionStage,
      materialStage:
        dto.materialStage === undefined
          ? undefined
          : choice(dto.materialStage, ['notStarted', 'pending', 'ready', 'unknown']),
      canonicalState: choice(dto.canonicalState, ['unknown', 'pending', 'prepared', 'applied', 'declined']),
      readiness: choice(dto.readiness, ['waiting', 'ready', 'synchronizing', 'done', 'settled']),
      errorCode: optionalText(dto.errorCode),
    };
  } catch {
    throw invalid();
  }
}
function sourceJson(value: ProducerSource) {
  const { fqdn, id } = value.eventId;
  return `{"eventId":{"fqdn":${JSON.stringify(fqdn)},"id":{"curr":${id.curr},"prev":${id.prev}}},"bodyCommitment":${JSON.stringify(value.bodyCommitment)}}`;
}
/** Deliberately emit only strict reference inputs, never observations or readiness. */
export function producerReferenceJson(receipt: ProducerReceipt): string {
  const decision = receipt.decisionStage;
  const json = `{"version":1,"kind":${JSON.stringify(receipt.kind)}${receipt.requestId ? `,"requestId":${JSON.stringify(receipt.requestId)}` : ''},"requestStage":${sourceJson(receipt.requestStage.source)}${decision?.state === 'attempted' ? `,"decisionStage":${sourceJson(decision.source)}` : ''}}`;
  if (new TextEncoder().encode(json).length > 4096) throw invalid();
  return json;
}
