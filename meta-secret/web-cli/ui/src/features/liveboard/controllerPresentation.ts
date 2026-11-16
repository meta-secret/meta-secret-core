import type { CoreOperation, CoreProjection } from './coreTypes';
import type { DeviceForecast, DeviceOperation, DeviceView } from './types';
import type { DeviceSlotPresentation, RequestVisual, VaultPresentation } from './presentation';
import type { DeepReadonly } from 'vue';

const storage = {
  whole: ['This device stores each secret in full, encrypted.', 'One device can recover the data.'],
  copies: ['Both devices store a complete encrypted copy.', 'Either device can recover the data.'],
  shares: ['Each device stores an encrypted share.', 'Any 2 of 3 can recover the data. One share alone cannot.'],
};
const forecastCopy = (count: number) =>
  count >= 3
    ? ['Each device would store an encrypted share.', 'Any 2 of 3 could recover the data. One share alone cannot.']
    : count === 2
      ? ['Both devices would store a complete encrypted copy.', 'Either device could recover the data.']
      : ['This device would store each secret in full, encrypted.', 'One device could recover the data.'];
export const currentDeviceId = (snapshot: CoreProjection) =>
  snapshot.devices.find((device) => device.current)?.deviceId;
export const provenResponse = (operation: CoreOperation) =>
  Boolean(operation.outcome && operation.responderId && operation.resolutionSource !== 'unknown');
const ownOperation = (snapshot: CoreProjection, operation: CoreOperation) =>
  operation.requesterId === currentDeviceId(snapshot);
export const resolvedRemoval = (operation: CoreOperation) =>
  operation.kind === 'removeDevice' &&
  provenResponse(operation) &&
  (operation.outcome === 'declined'
    ? ['declined', 'done'].includes(operation.clientStatus)
    : operation.outcome === 'accepted' &&
      operation.clientStatus === 'done' &&
      ['done', 'settled'].includes(operation.readiness));
export const completedRemoval = (snapshot: CoreProjection, operation: CoreOperation) =>
  snapshot.session.ready &&
  ownOperation(snapshot, operation) &&
  Boolean(operation.targetDeviceId) &&
  operation.outcome === 'accepted' &&
  resolvedRemoval(operation);

/** Numbers select hypothetical wording only; every real permission remains Core supplied. */
export function deviceForecasts(snapshot: CoreProjection): DeviceForecast[] {
  const removed = forecastCopy(snapshot.devices.length - 1);
  const forecasts: DeviceForecast[] = snapshot.devices
    .filter((device) => device.canRemove && !device.current)
    .map((device) => ({
      targetId: device.deviceId,
      mode: 'remove',
      storageTitle: removed[0]!,
      storageDetail: removed[1]!,
    }));
  for (let index = snapshot.devices.length; index < 3; index++) {
    const copy = forecastCopy(index + 1);
    forecasts.push({
      targetId: `empty-${index + 1}`,
      mode: 'connect',
      storageTitle: copy[0]!,
      storageDetail: copy[1]!,
    });
  }
  return forecasts;
}

/** Canonical display instruction, separate from a local producer's R/U correlation. */
export function deviceOperation(
  snapshot: CoreProjection,
  correlation?: { requestId?: string; subject: string; previousTerminals: ReadonlyMap<string, string> },
  localPending = false,
): DeviceOperation | null {
  if ((correlation && !correlation.requestId) || (localPending && !correlation)) return null;
  const candidates = snapshot.operations.filter(
    (operation) =>
      operation.kind === 'removeDevice' &&
      ownOperation(snapshot, operation) &&
      (!correlation || operation.targetDeviceId === correlation.subject) &&
      (!operation.outcome ||
        correlation?.previousTerminals.get(JSON.stringify([operation.kind, operation.requestId])) !==
          JSON.stringify([operation.kind, operation.requestId, operation.outcome])),
  );
  const operation = correlation?.requestId
    ? candidates.find((candidate) => candidate.requestId === correlation.requestId)
    : candidates.length === 1
      ? candidates[0]
      : undefined;
  if (!operation?.targetDeviceId || (operation.outcome && !provenResponse(operation))) return null;
  return {
    requestId: operation.requestId,
    targetId: operation.targetDeviceId,
    phase:
      operation.outcome === 'declined'
        ? resolvedRemoval(operation)
          ? 'declined'
          : 'settling'
        : operation.outcome === 'accepted'
          ? resolvedRemoval(operation)
            ? 'settled'
            : 'settling'
          : 'waiting',
    peers: [...operation.eligiblePeerIds],
    responderId: provenResponse(operation) ? operation.responderId : undefined,
  };
}

export const storageTitle = (snapshot: CoreProjection) =>
  snapshot.storageMode ? storage[snapshot.storageMode][0] : '';

export function requestVisual(snapshot: CoreProjection, operation?: CoreOperation): RequestVisual | null {
  if (
    !operation ||
    operation.kind === 'join' ||
    operation.kind === 'create' ||
    (operation.kind !== 'removeDevice' && operation.clientStatus === 'done') ||
    completedRemoval(snapshot, operation)
  )
    return null;
  // A log-only retirement ends white packets without inventing a returning device.
  if (operation.outcome && !provenResponse(operation)) return null;
  const secret = snapshot.secrets.find((item) => item.passId === operation.secretId);
  const target = snapshot.devices.find((item) => item.deviceId === operation.targetDeviceId);
  return {
    identity: JSON.stringify([operation.kind, operation.requestId]),
    kind:
      operation.kind === 'deleteSecret'
        ? 'delete-secret'
        : operation.kind === 'removeDevice'
          ? 'remove-device'
          : 'recover',
    phase: operation.outcome === 'accepted' ? 'approved' : operation.outcome === 'declined' ? 'declined' : 'waiting',
    requesterId: operation.requesterId,
    peerIds: provenResponse(operation)
      ? operation.responderId === operation.requesterId
        ? []
        : [operation.responderId!]
      : [...operation.eligiblePeerIds],
    responderId: provenResponse(operation) ? operation.responderId : undefined,
    title:
      operation.kind === 'recover'
        ? 'Secret recovery'
        : operation.kind === 'deleteSecret'
          ? 'Delete secret'
          : 'Remove device',
    subject: secret?.name ?? target?.name ?? '',
    detail:
      operation.outcome === 'accepted'
        ? 'Request approved.'
        : operation.outcome === 'declined'
          ? 'Request declined.'
          : 'Waiting for a response.',
  };
}

export function vaultPresentation(input: {
  snapshot: CoreProjection;
  devices: DeepReadonly<DeviceView>;
  request: RequestVisual | null;
}): VaultPresentation {
  const { snapshot, devices, request } = input;
  const actual = [
    ...snapshot.devices.filter((device) => device.current),
    ...snapshot.devices.filter((device) => !device.current),
  ];
  const slot = (index: number): DeviceSlotPresentation => {
    const device = actual[index];
    const slotId = device?.deviceId ?? `empty-${index + 1}`;
    const preview = devices.preview?.targetId === slotId;
    return {
      device: device ? { ...device } : undefined,
      slotId,
      heading: `Device ${String(index + 1).padStart(2, '0')}`,
      art: device
        ? ['Android', 'Iphone'].includes(device.category)
          ? 'phone'
          : device.category === 'Tablet'
            ? 'tablet'
            : 'laptop'
        : 'placeholder',
      name: device?.name ?? 'Connect a device',
      state: device?.current ? 'This device' : device ? 'Connected' : preview ? 'Preview' : '',
      preview,
      removed: preview && devices.preview?.mode === 'remove',
      requester: Boolean(device && request && device.deviceId === request.requesterId),
      responder: Boolean(device && device.deviceId === request?.responderId),
      peer: Boolean(device && request?.peerIds.includes(device.deviceId)),
      disabled: !snapshot.session.ready || devices.busy || Boolean(device && (device.current || !device.canRemove)),
    };
  };
  const removable = Boolean(
    devices.preview?.mode === 'remove' &&
    snapshot.devices.some(
      (device) => device.deviceId === devices.preview?.targetId && device.canRemove && !device.current,
    ),
  );
  return {
    slots: [slot(0), slot(1), slot(2)],
    storage: {
      label: devices.preview ? 'Time Machine' : 'Storage model',
      title: devices.preview?.storageTitle ?? storageTitle(snapshot),
      detail: devices.preview?.storageDetail ?? (snapshot.storageMode ? storage[snapshot.storageMode][1] : ''),
      preview: Boolean(devices.preview),
      removable,
      busy: devices.busy || !snapshot.session.ready,
    },
    request,
    canInvite: snapshot.session.ready && Boolean(snapshot.vaultName),
  };
}
