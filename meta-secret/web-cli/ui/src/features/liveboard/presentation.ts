import type { DeviceDescriptor, DeviceView } from './types';
import type { liveboardGeometry } from '@/design/liveboard/geometry';

export interface DeviceSlotPresentation {
  device?: DeviceDescriptor;
  slotId: string;
  heading: string;
  art: keyof typeof liveboardGeometry.art;
  name: string;
  state: string;
  preview?: boolean;
  removed?: boolean;
  requester?: boolean;
  responder?: boolean;
  peer?: boolean;
  disabled?: boolean;
}
export interface RequestVisual {
  identity: string;
  kind: 'recover' | 'delete-secret' | 'remove-device';
  phase: 'waiting' | 'approved' | 'declined';
  requesterId: string;
  peerIds: readonly string[];
  responderId?: string;
  title: string;
  subject: string;
  detail: string;
}
export interface VaultPresentation {
  slots: readonly [DeviceSlotPresentation, DeviceSlotPresentation, DeviceSlotPresentation];
  storage: { label: string; title: string; detail: string; preview: boolean; removable: boolean; busy: boolean };
  request: RequestVisual | null;
  canInvite: boolean;
}
export interface ProfilePresentation {
  vaultName: string;
  deviceCount: number;
  secretCount: number;
  version: string;
  alpha: boolean;
  resetDescription: string;
}
export type DeviceState = Readonly<DeviceView>;

/** Supplied only after canonical confirmation; absence of a queued item is not a receipt. */
export interface NotificationReceiptPresentation {
  identity: string;
  outcome: 'approved' | 'declined' | 'handled';
  requesterName: string;
  subjectName?: string;
}
