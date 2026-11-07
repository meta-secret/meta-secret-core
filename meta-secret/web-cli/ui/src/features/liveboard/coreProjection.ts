import type { WasmUserMemberFullInfo } from 'meta-secret-web-cli';
import type { CoreOperation, CoreProjection } from './coreTypes';
import type { SecretPrimary, RequestKind } from './types';
import { array, boolean, choice, invalid, optionalText, record, text, unique } from './coreValidation';
import { ownedScope } from './ownedScope';

const kinds = ['join', 'create', 'recover', 'deleteSecret', 'removeDevice'] as const;
const statuses = ['pending', 'needApprove', 'accepted', 'declined', 'done', 'unknown', 'synchronizing'] as const;
const readiness = ['waiting', 'ready', 'synchronizing', 'done', 'settled'] as const;
const requestKinds: Partial<Record<CoreOperation['kind'], RequestKind>> = {
  join: 'join',
  recover: 'recover',
  deleteSecret: 'delete-secret',
  removeDevice: 'remove-device',
};

function decodeOperation(value: unknown): CoreOperation {
  const op = record(value);
  const eligiblePeerIds = array(op.eligiblePeerIds, text);
  unique(eligiblePeerIds);
  return {
    kind: choice(op.kind, kinds),
    requestId: text(op.requestId),
    requesterId: text(op.requesterId),
    targetDeviceId: optionalText(op.targetDeviceId),
    secretId: optionalText(op.secretId),
    clientStatus: choice(op.clientStatus, statuses),
    eligiblePeerIds,
    canApprove: boolean(op.canApprove),
    canDecline: boolean(op.canDecline),
    outcome: op.outcome === undefined ? undefined : choice(op.outcome, ['accepted', 'declined']),
    responderId: optionalText(op.responderId),
    resolutionSource: choice(op.resolutionSource, ['unknown', 'canonical', 'verified']),
    readiness: choice(op.readiness, readiness),
  };
}
function decodeSecret(value: unknown) {
  const secret = record(value);
  return {
    passId: text(secret.secretId),
    action: choice(secret.primaryAction, ['showLocal', 'showRecovered', 'recover', 'waiting', 'unavailable']),
    readiness: choice(secret.readiness, ['ready', 'waiting', 'synchronizing']),
    claimId: optionalText(secret.recoveryClaimId),
    deletionRequestId: optionalText(secret.deletionRequestId),
    canDelete: boolean(secret.canDelete),
  };
}
function primary(secret: ReturnType<typeof decodeSecret>): SecretPrimary {
  if ((secret.action === 'showRecovered' || secret.action === 'waiting') && !secret.claimId) throw invalid();
  if (secret.action === 'waiting' && secret.readiness === 'waiting')
    return { kind: 'waiting', claimId: secret.claimId! };
  if (secret.readiness !== 'ready') return { kind: 'blocked' };
  if (secret.action === 'showLocal') return { kind: 'show-local' };
  if (secret.action === 'showRecovered') return { kind: 'show-recovered', claimId: secret.claimId! };
  return { kind: secret.action === 'recover' ? 'recover' : 'blocked' };
}
export function blockedProjection(identity = ''): CoreProjection {
  return {
    session: { identity, vaultId: '', member: false, ready: false },
    vaultName: '',
    storageMode: null,
    canCreateSecret: false,
    secrets: [],
    requests: [],
    operations: [],
    devices: [],
  };
}

/** All display joins and permissions are copied from this single member snapshot. */
export function readCoreProjection({
  member,
  identity,
}: {
  member: WasmUserMemberFullInfo;
  identity: string;
}): CoreProjection {
  const scope = ownedScope();
  try {
    if (typeof member.liveboard_state_json !== 'function') throw invalid();
    const dto = record(JSON.parse(member.liveboard_state_json()));
    if (dto.schemaVersion !== 1) throw invalid();
    const vaultName = text(dto.vaultName),
      currentId = text(dto.currentDeviceId);
    const storageMode = choice(dto.storageMode, ['whole', 'copies', 'shares']);
    const canCreateSecret = boolean(dto.canCreateSecret);
    const removable = array(dto.removableDeviceIds, text);
    unique(removable);
    const decodedSecrets = array(dto.secrets, decodeSecret);
    const operations = array(dto.operations, decodeOperation);
    unique(decodedSecrets.map((secret) => secret.passId));
    unique(operations.map((op) => JSON.stringify([op.kind, op.requestId])));
    const vault = scope.own(member.vault_data());
    if (scope.own(vault.vault_name())[0] !== vaultName) throw invalid();
    const passes = vault.secrets().map(scope.own);
    const names = new Map(passes.map((pass) => [pass.id_str(), pass.name]));
    unique(passes.map((pass) => pass.id_str()));
    const users = vault.users().map(scope.own);
    const labels = new Map<string, string>();
    const devices: CoreProjection['devices'] = [];
    for (const user of users) {
      const data = scope.own(user.user_data()),
        device = scope.own(data.device);
      const id = scope.own(device.device_id).wasm_id_str();
      const name = scope.own(device.device_name).as_str();
      if (!id || typeof name !== 'string' || labels.has(id)) throw invalid();
      labels.set(id, name);
      if (user.is_member())
        devices.push({
          deviceId: id,
          name,
          current: id === currentId,
          canRemove: id !== currentId && removable.includes(id),
          category: ['Android', 'Iphone', 'Tablet', 'Desktop', 'Cli', 'Web', 'Other'][device.ui_category()] ?? 'Other',
        });
    }
    if (!devices.some((device) => device.current)) throw invalid();
    const secrets = decodedSecrets.map((secret) => {
      const name = names.get(secret.passId);
      if (typeof name !== 'string') throw invalid();
      return {
        passId: secret.passId,
        name,
        primary: primary(secret),
        canDelete: secret.canDelete,
        deletionRequestId: secret.deletionRequestId,
      };
    });
    const requests = operations.flatMap((op) => {
      const kind = requestKinds[op.kind];
      if (!kind || (!op.canApprove && !op.canDecline)) return [];
      return [
        {
          kind,
          requestId: op.requestId,
          requesterId: op.requesterId,
          requesterName: labels.get(op.requesterId) ?? '',
          subjectId: op.secretId ?? op.targetDeviceId,
          subjectName: op.secretId ? names.get(op.secretId) : labels.get(op.targetDeviceId ?? ''),
          canApprove: op.canApprove,
          canDecline: op.canDecline,
        },
      ];
    });
    return {
      session: { identity, vaultId: vaultName, member: true, ready: true },
      vaultName,
      storageMode,
      canCreateSecret,
      secrets,
      requests,
      operations,
      devices,
    };
  } catch {
    return blockedProjection(identity);
  } finally {
    scope.release();
  }
}
