/** Presentation contracts only. Core owns every eligibility, peer and result input. */
export interface LiveboardSession {
  identity: string;
  vaultId: string;
  member: boolean;
  ready: boolean;
}

export type RequestKind = 'join' | 'recover' | 'delete-secret' | 'remove-device';
export interface RequestKey {
  kind: RequestKind;
  requestId: string;
}
export interface IncomingRequest extends RequestKey {
  requesterId: string;
  requesterName: string;
  subjectId?: string;
  subjectName?: string;
  canApprove: boolean;
  canDecline: boolean;
}
export interface DecisionIntent extends RequestKey {
  accepted: boolean;
}
export interface OperationReceipt {
  operationId: string;
  phase: 'submitted' | 'unknown' | 'confirmed' | 'rejected';
}
export interface NotificationSnapshot {
  session: LiveboardSession;
  requests: readonly IncomingRequest[];
}
export interface NotificationPort {
  authenticate(intent: DecisionIntent): Promise<boolean>;
  decide(intent: DecisionIntent): Promise<OperationReceipt>;
}
export interface NotificationView {
  pendingDecision?: DecisionIntent | null;
  requests: IncomingRequest[];
  selected: RequestKey | null;
  expanded: boolean;
  busy: boolean;
  error: string | null;
}

export type SecretPrimary =
  | { kind: 'show-local' }
  | { kind: 'show-recovered'; claimId: string }
  | { kind: 'recover' }
  | { kind: 'waiting'; claimId: string }
  | { kind: 'blocked' };
export interface SecretDescriptor {
  passId: string;
  name: string;
  primary: SecretPrimary;
  canDelete: boolean;
  deletionRequestId?: string;
}
export type RevealedValue = { kind: 'text'; text: string } | { kind: 'seed'; words: readonly string[] };
/** An unfinished explicit Show owner; object identity, never a Core permission or receipt. */
export interface RecoveredRevealOwner {
  readonly passId: string;
  readonly claimId: string;
  isCurrent(): boolean;
  onRevoke(listener: () => void): () => void;
}
export type RecoveredRevealResult = RevealedValue | { kind: 'delivered'; owner: RecoveredRevealOwner };
export interface SecretCardView {
  conceal?: boolean;
  secret: SecretDescriptor;
  phase: 'masked' | 'revealing' | 'revealed' | 'waiting' | 'deleting';
  value: RevealedValue | null;
  copied: boolean;
}
export interface SecretPresentation {
  active: boolean;
  visibleIds: readonly string[];
  query: string;
  scrollTop: number;
}
export interface SecretSnapshot {
  session: LiveboardSession;
  secrets: readonly SecretDescriptor[];
}
export interface SecretPort {
  showLocal(passId: string): Promise<RevealedValue>;
  showRecovered(intent: { passId: string; claimId: string }): Promise<RecoveredRevealResult>;
  recover(passId: string): Promise<OperationReceipt>;
  deleteSecret(passId: string): Promise<OperationReceipt>;
  authenticate(intent: { kind: 'delete-secret'; passId: string }): Promise<boolean>;
  copy(text: string): Promise<void>;
  reportError(message: string): void;
}
export interface SecretView {
  cards: Record<string, SecretCardView>;
  presentation: SecretPresentation;
  blocked: boolean;
}

export interface CreateDraft {
  mode: 'secret' | 'seed';
  name: string;
  value: string;
  wordCount: 12 | 24;
  words: string[];
}
export type CreateReceipt =
  | { operationId: string; phase: 'submitted' | 'unknown' | 'rejected' }
  | { operationId: string; phase: 'confirmed'; passId: string };
export interface CreatePort {
  create(draft: Readonly<CreateDraft>): Promise<CreateReceipt>;
  reconcile(operationId: string): Promise<CreateReceipt>;
  readClipboard(): Promise<string>;
  reportError(message: string): void;
}
export interface CreateView {
  open: boolean;
  draft: CreateDraft;
  phase: 'idle' | 'saving' | 'reconciling';
  receipt: CreateReceipt | null;
  pasteHint: string;
}

export interface DeviceDescriptor {
  deviceId: string;
  name: string;
  current: boolean;
  canRemove: boolean;
}
export interface DeviceForecast {
  targetId: string;
  mode: 'connect' | 'remove';
  storageTitle: string;
  storageDetail: string;
}
export interface DeviceOperation {
  requestId: string;
  targetId: string;
  phase: 'waiting' | 'declined' | 'settling' | 'settled';
  peers: readonly string[];
  responderId?: string;
}
export interface DeviceSnapshot {
  session: LiveboardSession;
  devices: readonly DeviceDescriptor[];
  forecasts: readonly DeviceForecast[];
  storageTitle: string;
  operation: DeviceOperation | null;
}
export interface DevicePort {
  authenticate(targetId: string): Promise<boolean>;
  remove(targetId: string): Promise<OperationReceipt>;
  reportError(message: string): void;
}
export interface DeviceView {
  devices: DeviceDescriptor[];
  preview: DeviceForecast | null;
  operation: DeviceOperation | null;
  storageTitle: string;
  busy: boolean;
}
