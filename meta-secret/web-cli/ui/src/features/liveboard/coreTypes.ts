import type { WasmApplicationManager, WasmUserMemberFullInfo } from 'meta-secret-web-cli';
import type {
  CreatePort,
  DeviceDescriptor,
  DevicePort,
  IncomingRequest,
  LiveboardSession,
  NotificationPort,
  SecretDescriptor,
  SecretPort,
} from './types';

export type CoreKind = 'join' | 'create' | 'recover' | 'deleteSecret' | 'removeDevice';
export interface CoreOperation {
  kind: CoreKind;
  requestId: string;
  requesterId: string;
  targetDeviceId?: string;
  secretId?: string;
  clientStatus: string;
  eligiblePeerIds: string[];
  canApprove: boolean;
  canDecline: boolean;
  outcome?: 'accepted' | 'declined';
  responderId?: string;
  resolutionSource: 'unknown' | 'canonical' | 'verified';
  readiness: string;
}
export interface CoreProjection {
  session: LiveboardSession;
  vaultName: string;
  storageMode: 'whole' | 'copies' | 'shares' | null;
  canCreateSecret: boolean;
  secrets: SecretDescriptor[];
  requests: IncomingRequest[];
  operations: CoreOperation[];
  devices: Array<DeviceDescriptor & { category: string }>;
}
export interface CoreContext {
  identity: string;
  epoch: number;
  ready: boolean;
  member: WasmUserMemberFullInfo;
  manager: WasmApplicationManager;
}
export interface CorePlatform {
  authenticate(): Promise<boolean>;
  copy(text: string): Promise<void>;
  readClipboard(): Promise<string>;
  reportError(message: string): void;
}
export interface CheckedReceipt {
  kind: CoreKind;
  secretId?: string;
  claimId?: string;
  requestId?: string;
  phase: 'queued' | 'unknown';
  errorCode?: string;
}
export interface ProducerSource {
  eventId: { fqdn: { objType: string; objInstance: string }; id: { curr: bigint; prev: bigint } };
  bodyCommitment: string;
}
export interface ProducerReceipt {
  version: 1;
  kind: 'deleteSecret' | 'removeDevice';
  phase: 'queued' | 'unknown';
  secretId?: string;
  targetDeviceId?: string;
  requestId?: string;
  requestStage: { source: ProducerSource; observation: string };
  decisionStage?: { state: 'notAttempted' } | { state: 'attempted'; source: ProducerSource; observation: string };
  materialStage?: string;
  canonicalState: string;
  readiness: string;
  errorCode?: string;
}
export interface CoreAdapter {
  project(): CoreProjection;
  notifications: NotificationPort;
  secrets: SecretPort;
  create: CreatePort;
  devices: DevicePort;
  reconcileProducer(operationId: string): Promise<ProducerReceipt>;
  continueRemove(operationId: string): Promise<ProducerReceipt>;
}
