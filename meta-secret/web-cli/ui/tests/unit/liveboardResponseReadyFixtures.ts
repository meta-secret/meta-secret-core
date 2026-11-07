import { expect, vi } from 'vitest';
import type { UserData, WasmApplicationState, WasmUserMembership } from 'meta-secret-web-cli';
import VaultTile from '@/components/liveboard/VaultTile.vue';
import { device, disposable, operation, pass, requestId } from './liveboardCoreFixtures';
import {
  before as existingBefore,
  localDTO,
  noBusiness,
  settle,
  setup as existingSetup,
  type Setup,
} from './liveboardManualErrorReadinessFixtures';

export {
  after,
  back,
  concealed,
  copyError,
  dismiss,
  noBusiness,
  rejectedCopy,
  settle,
  show,
  tick,
} from './liveboardManualErrorReadinessFixtures';
export { deferred, managerFixture, operation, producer, requestId } from './liveboardCoreFixtures';
export type { Setup } from './liveboardManualErrorReadinessFixtures';
export const ownId = 'claim-second';
export const firstId = 'claim-first';
export const secondId = requestId;
export function before() {
  existingBefore();
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockImplementation(function (this: HTMLElement) {
    const id = this.closest<HTMLElement>('[data-device-id]')?.dataset.deviceId;
    return new DOMRect(id === 'peer-a' ? 180 : id === 'peer-b' ? 320 : 30, 0, id ? 80 : 500, 160);
  });
}
function thirdMember(root: WasmApplicationState) {
  const data = root.as_vault().as_member().vault_data();
  const users = data.users();
  const peer = users[1]!;
  const user = peer.user_data();
  const thirdUser: UserData = {
    ...disposable(),
    vault_name: user.vault_name,
    device: {
      ...disposable(),
      device_id: device('peer-b'),
      device_name: user.device.device_name,
      device_type: user.device.device_type,
      keys: user.device.keys,
      ui_category: () => 5,
    },
  };
  const third: WasmUserMembership = {
    ...disposable(),
    user_data: () => thirdUser,
    is_member: () => true,
    is_outsider: () => false,
    as_member: () => ({ ...disposable(), user_data: thirdUser }),
    as_outsider: vi.fn<WasmUserMembership['as_outsider']>(),
  };
  vi.mocked(data.users).mockImplementation(() => [...users, third]);
  vi.mocked(data.secrets).mockImplementation(() => [
    pass('pass-a', 'Alpha'),
    pass('pass-b', 'Beta'),
    pass('pass-c', 'Gamma'),
  ]);
  return root;
}
export async function setup(path = '/') {
  const s = await existingSetup(path);
  const read = vi.mocked(s.manager.get_state).getMockImplementation()!;
  vi.mocked(s.manager.get_state).mockImplementation(async () => thirdMember(await read()));
  return s;
}
export const own = (patch = {}) =>
  operation({
    requestId: ownId,
    requesterId: 'self',
    eligiblePeerIds: ['peer-a', 'peer-b'],
    canApprove: false,
    canDecline: false,
    clientStatus: 'pending',
    ...patch,
  });
export const payload = (operations = [own()], secretPatch = {}) => ({
  ...localDTO(),
  storageMode: 'shares',
  removableDeviceIds: ['peer-a', 'peer-b'],
  operations,
  secrets: ['pass-a', 'pass-b', 'pass-c'].map((secretId) => ({
    secretId,
    primaryAction: 'recover',
    canDelete: true,
    readiness: 'ready',
    ...(secretId === 'pass-a' ? secretPatch : {}),
  })),
});
export const pending = () =>
  payload([own()], { primaryAction: 'waiting', recoveryClaimId: ownId, readiness: 'waiting' });
export const response = (accepted = true, patch = {}) =>
  payload(
    [
      own({
        clientStatus: accepted ? 'accepted' : 'declined',
        outcome: accepted ? 'accepted' : 'declined',
        responderId: 'peer-b',
        resolutionSource: 'verified',
        eligiblePeerIds: [],
        readiness: accepted ? 'ready' : 'settled',
        ...patch,
      }),
    ],
    accepted
      ? { primaryAction: 'showRecovered', recoveryClaimId: ownId, readiness: 'ready' }
      : { primaryAction: 'recover' },
  );
export const mixed = () => {
  const dto = payload([
    operation({ requestId: firstId, secretId: 'pass-b' }),
    operation({ kind: 'deleteSecret', requestId: secondId, secretId: 'pass-c' }),
  ]);
  dto.secrets[2] = { ...dto.secrets[2]!, canDelete: false, ...{ deletionRequestId: secondId } };
  return dto;
};
export async function fresh(s: Setup, dto: ReturnType<typeof localDTO>) {
  const reads = vi.mocked(s.manager.get_state).mock.calls.length;
  const syncs = vi.mocked(s.manager.sync_now).mock.calls.length;
  s.setDTO(dto);
  await s.session.refresh({ requiredSync: true });
  await settle();
  expect(s.manager.sync_now).toHaveBeenCalledTimes(syncs + 1);
  expect(s.manager.get_state).toHaveBeenCalledTimes(reads + 1);
  expect(s.session.state.value.ready).toBe(true);
  expect(s.physical.maximum).toBe(1);
}
/** Arrange an already observed baseline before the ONE response read under test.
 * Ordinary real route reads here are setup only, never a post-response rescue. */
export async function observedBaseline(s: Setup, dto: ReturnType<typeof localDTO>) {
  await fresh(s, dto);
  await s.router.push('/profile');
  await settle();
  await s.router.push('/');
  await settle();
  expect(s.session.state.value.ready).toBe(true);
}
export const vault = (s: Setup) => s.wrapper.getComponent(VaultTile).props('vault');
export function passive(s: Setup) {
  expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
  expect(s.manager.show_local_secret).not.toHaveBeenCalled();
  expect(s.manager.show_recovered).not.toHaveBeenCalled();
  expect(s.clipboard.writeText).not.toHaveBeenCalled();
  noBusiness(s);
}
export function head(s: Setup, kind: string, id: string, count: number) {
  expect(s.wrapper.get('.notification-count').text()).toBe(String(count));
  expect(s.wrapper.get('.notification-actions').attributes('data-request-kind')).toBe(kind);
  expect(s.wrapper.get('.notification-actions').attributes('data-request-id')).toBe(id);
}
export const open = async (s: Setup) => {
  await s.wrapper.get('.notification-hit').trigger('click');
  await settle();
};
export const marker = (s: Setup, label: string) => {
  s.stages.push(label);
  console.info('RESPONSE_READY_STAGE', label);
};
