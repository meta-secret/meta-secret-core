import { vi } from 'vitest';
import type {
  CreatePort,
  DevicePort,
  DeviceSnapshot,
  IncomingRequest,
  LiveboardSession,
  NotificationPort,
  RequestKind,
  SecretDescriptor,
  SecretPort,
} from '@/features/liveboard/types';

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export const session = (patch: Partial<LiveboardSession> = {}): LiveboardSession => ({
  identity: 'session-a',
  vaultId: 'vault-a',
  member: true,
  ready: true,
  ...patch,
});
export const request = (
  kind: RequestKind = 'join',
  requestId = 'request-a',
  patch: Partial<IncomingRequest> = {},
): IncomingRequest => ({
  kind,
  requestId,
  requesterId: 'peer-a',
  requesterName: 'Same device name',
  canApprove: true,
  canDecline: true,
  ...patch,
});
export const secret = (passId = 'pass-a', patch: Partial<SecretDescriptor> = {}): SecretDescriptor => ({
  passId,
  name: 'Same secret name',
  primary: { kind: 'show-local' },
  canDelete: true,
  ...patch,
});
export const notificationPort = (patch: Partial<NotificationPort> = {}): NotificationPort => ({
  authenticate: vi.fn(async () => true),
  decide: vi.fn(async () => ({ operationId: 'operation-a', phase: 'submitted' as const })),
  ...patch,
});
export const secretPort = (patch: Partial<SecretPort> = {}): SecretPort => ({
  showLocal: vi.fn(async () => ({ kind: 'text' as const, text: 'private value' })),
  showRecovered: vi.fn(async () => ({ kind: 'text' as const, text: 'recovered value' })),
  recover: vi.fn(async () => ({ operationId: 'claim-a', phase: 'submitted' as const })),
  deleteSecret: vi.fn(async () => ({ operationId: 'delete-a', phase: 'submitted' as const })),
  authenticate: vi.fn(async () => true),
  copy: vi.fn(async () => {}),
  reportError: vi.fn(),
  ...patch,
});
export const createPort = (patch: Partial<CreatePort> = {}): CreatePort => ({
  create: vi.fn(async () => ({ operationId: 'create-a', phase: 'submitted' as const })),
  reconcile: vi.fn(async () => ({ operationId: 'create-a', phase: 'unknown' as const })),
  readClipboard: vi.fn(async () => 'one two three four five six seven eight nine ten eleven twelve'),
  reportError: vi.fn(),
  ...patch,
});
export const devicePort = (patch: Partial<DevicePort> = {}): DevicePort => ({
  authenticate: vi.fn(async () => true),
  remove: vi.fn(async () => ({ operationId: 'remove-a', phase: 'submitted' as const })),
  reportError: vi.fn(),
  ...patch,
});
export const deviceSnapshot = (patch: Partial<DeviceSnapshot> = {}): DeviceSnapshot => ({
  session: session(),
  devices: [
    { deviceId: 'self', name: 'Current phone', current: true, canRemove: false },
    { deviceId: 'peer-a', name: 'Tablet', current: false, canRemove: true },
    { deviceId: 'peer-b', name: 'Web', current: false, canRemove: true },
  ],
  forecasts: [
    { targetId: 'peer-a', mode: 'remove', storageTitle: 'Full encrypted copies', storageDetail: 'Core forecast' },
  ],
  storageTitle: 'Encrypted shares',
  operation: null,
  ...patch,
});
