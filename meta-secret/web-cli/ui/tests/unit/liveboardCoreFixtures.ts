import { vi } from 'vitest';
import type {
  ClaimId,
  DeviceData,
  DeviceId,
  DeviceName,
  DeviceType,
  MetaPasswordId,
  UserData,
  UserDataMember,
  WasmApplicationManager,
  WasmApplicationState,
  WasmUserMemberFullInfo,
  WasmUserMembership,
  WasmVaultData,
  WasmVaultFullInfo,
} from 'meta-secret-web-cli';
import { deferred } from './liveboardFixtures';

export { deferred };
export const requestId = 'd732b6ac-0881-42ac-abd6-a5cba32b34ec';
export const secondRequestId = 'd732b6ac-0881-42ac-abd6-a5cba32b34ed';
export const disposable = () => ({ free: vi.fn(), [Symbol.dispose]: vi.fn() });
const encoded = () => ({ ...disposable(), text: { ...disposable() } });
export const pass = (id = 'pass-a', name = 'Same secret name'): MetaPasswordId => ({
  ...disposable(),
  id_str: vi.fn(() => id),
  name,
  id: encoded(),
});
export const claim = (id = 'claim-second'): ClaimId => ({
  ...disposable(),
  wasm_id_str: vi.fn(() => id),
  0: { ...disposable(), text: id },
});
export const device = (id = 'peer-a'): DeviceId => ({
  ...disposable(),
  wasm_id_str: vi.fn(() => id),
  0: encoded(),
});
function user(id: string, name: string): WasmUserMembership {
  const deviceName: DeviceName = { ...disposable(), as_str: () => name };
  const deviceType: DeviceType = { ...disposable(), as_str: () => 'Web' };
  const data: DeviceData = {
    ...disposable(),
    device_id: device(id),
    device_name: deviceName,
    device_type: deviceType,
    ui_category: () => 5,
    keys: { ...disposable(), dsa_pk: { ...disposable(), 0: disposable() }, transport_pk: disposable() },
  };
  const userData: UserData = {
    ...disposable(),
    device: data,
    vault_name: { ...disposable(), 0: 'семья@example.test' },
  };
  const member: UserDataMember = { ...disposable(), user_data: userData };
  return {
    ...disposable(),
    user_data: () => userData,
    is_member: () => true,
    is_outsider: () => false,
    as_member: () => member,
    as_outsider: vi.fn<WasmUserMembership['as_outsider']>(),
  };
}

export const operation = (patch = {}) => ({
  kind: 'recover',
  requestId: 'claim-second',
  requesterId: 'peer-a',
  secretId: 'pass-a',
  clientStatus: 'needApprove',
  eligiblePeerIds: ['self'],
  canApprove: true,
  canDecline: true,
  resolutionSource: 'unknown',
  readiness: 'waiting',
  ...patch,
});
export const coreState = (patch = {}) => ({
  schemaVersion: 1,
  vaultName: 'семья@example.test',
  currentDeviceId: 'self',
  storageMode: 'whole',
  canCreateSecret: true,
  removableDeviceIds: ['peer-a'],
  secrets: [{ secretId: 'pass-a', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' }],
  operations: [operation()],
  ...patch,
});
export function memberFixture(json = JSON.stringify(coreState())) {
  const vaultData: WasmVaultData = {
    ...disposable(),
    users: vi.fn(() => [user('self', 'Current Web'), user('peer-a', 'Same device name')]),
    members: vi.fn(() => [user('self', 'Current Web').as_member(), user('peer-a', 'Same device name').as_member()]),
    outsiders: vi.fn(() => []),
    secrets: vi.fn(() => [pass(), pass('pass-b')]),
    vault_name: () => ({ ...disposable(), 0: 'семья@example.test' }),
  };
  const member: WasmUserMemberFullInfo = {
    ...disposable(),
    liveboard_state_json: vi.fn(() => json),
    vault_data: vi.fn(() => vaultData),
    recovery_claim_by_id: vi.fn((id: string) => (['claim-first', 'claim-second'].includes(id) ? claim(id) : undefined)),
    deletion_states_json: vi.fn(() => '[]'),
    pending_removal_targets: vi.fn(() => []),
    find_pending_incoming_recovery_claim: vi.fn<WasmUserMemberFullInfo['find_pending_incoming_recovery_claim']>(),
    find_recovery_claim: vi.fn<WasmUserMemberFullInfo['find_recovery_claim']>(),
    pending_incoming_recovery_claim_count: vi.fn(() => 2),
    pending_incoming_recovery_claim_sender_type: vi.fn(() => 'Web'),
    recovery_client_status: vi.fn(() => 'NeedApprove'),
  };
  return { member, vaultData };
}
export function stateFixture(member = memberFixture().member) {
  const vault: WasmVaultFullInfo = {
    ...disposable(),
    as_member: () => member,
    vault_name: () => 'семья@example.test',
    is_member: () => true,
    is_outsider: () => false,
    is_vault_not_exists: () => false,
    as_outsider: vi.fn<WasmVaultFullInfo['as_outsider']>(),
    as_not_exists: vi.fn<WasmVaultFullInfo['as_not_exists']>(),
  };
  const state: WasmApplicationState = {
    ...disposable(),
    as_info: () => 1,
    is_vault: () => true,
    is_local: () => false,
    as_vault: () => vault,
    device_id: () => device('self'),
    as_local: vi.fn<WasmApplicationState['as_local']>(),
  };
  return { state, vault, member };
}
export const checked = (patch = {}) =>
  JSON.stringify({
    kind: 'recover',
    secretId: 'pass-a',
    claimId: 'claim-second',
    phase: 'queued',
    ...patch,
  });

// Raw JSON deliberately retains numeric tokens beyond Number precision.
export function producer(
  options: { kind?: 'deleteSecret' | 'removeDevice'; curr?: string; decision?: boolean; request?: boolean } = {},
) {
  const { kind = 'deleteSecret', curr = '18446744073709551615', decision = false, request = true } = options;
  const source = (n: string, commitment: string) =>
    `{"eventId":{"fqdn":{"objType":"DeviceLog","objInstance":"self"},"id":{"curr":${n},"prev":${BigInt(n) - 1n}}},"bodyCommitment":"${commitment.repeat(64)}"}`;
  const r = source(curr, 'a');
  const u = source('9007199254740993', 'b');
  return `{"version":1,"kind":"${kind}","phase":"unknown",${kind === 'deleteSecret' ? '"secretId":"pass-a",' : '"targetDeviceId":"peer-a",'}${request ? `"requestId":"${requestId}",` : ''}"requestStage":{"source":${r},"observation":"persisted"}${decision ? `,"decisionStage":{"state":"attempted","source":${u},"observation":"unconfirmed"}` : ''},"canonicalState":"unknown","readiness":"waiting"}`;
}
export function managerFixture() {
  const manager: WasmApplicationManager = {
    ...disposable(),
    accept_recover: vi.fn<WasmApplicationManager['accept_recover']>().mockResolvedValue(),
    decline_recover: vi.fn<WasmApplicationManager['decline_recover']>().mockResolvedValue(),
    approve_remove_member: vi.fn<WasmApplicationManager['approve_remove_member']>().mockResolvedValue(),
    decline_remove_member: vi.fn<WasmApplicationManager['decline_remove_member']>().mockResolvedValue(),
    clean_up_database: vi.fn<WasmApplicationManager['clean_up_database']>().mockResolvedValue(),
    cluster_distribution: vi.fn<WasmApplicationManager['cluster_distribution']>().mockResolvedValue(),
    cluster_distribution_checked: vi
      .fn<WasmApplicationManager['cluster_distribution_checked']>()
      .mockResolvedValue(checked({ kind: 'create', claimId: undefined })),
    continue_local_remove_checked: vi
      .fn<WasmApplicationManager['continue_local_remove_checked']>()
      .mockResolvedValue(producer({ kind: 'removeDevice', decision: true })),
    decide_delete_secret: vi.fn<WasmApplicationManager['decide_delete_secret']>().mockResolvedValue(),
    decide_join_request: vi
      .fn<WasmApplicationManager['decide_join_request']>()
      .mockResolvedValue(checked({ kind: 'join', requestId, claimId: undefined, secretId: undefined })),
    decide_remove_member: vi
      .fn<WasmApplicationManager['decide_remove_member']>()
      .mockResolvedValue(checked({ kind: 'removeDevice', requestId, claimId: undefined, secretId: undefined })),
    delete_secret: vi.fn<WasmApplicationManager['delete_secret']>().mockResolvedValue(),
    delete_secret_checked: vi.fn<WasmApplicationManager['delete_secret_checked']>().mockResolvedValue(producer()),
    find_claim_by_pass_id: vi.fn<WasmApplicationManager['find_claim_by_pass_id']>(),
    generate_user_creds: vi.fn<WasmApplicationManager['generate_user_creds']>().mockResolvedValue(),
    get_state: vi.fn<WasmApplicationManager['get_state']>().mockResolvedValue(stateFixture().state),
    join_result_auth_token: vi.fn<WasmApplicationManager['join_result_auth_token']>().mockResolvedValue('join-token'),
    reconcile_producer_receipt: vi
      .fn<WasmApplicationManager['reconcile_producer_receipt']>()
      .mockResolvedValue(producer()),
    recover_checked: vi.fn<WasmApplicationManager['recover_checked']>().mockResolvedValue(checked()),
    recover_js: vi.fn<WasmApplicationManager['recover_js']>().mockResolvedValue(),
    remove_member_checked: vi
      .fn<WasmApplicationManager['remove_member_checked']>()
      .mockResolvedValue(producer({ kind: 'removeDevice', request: false })),
    remove_member_from_vault: vi.fn<WasmApplicationManager['remove_member_from_vault']>().mockResolvedValue(),
    show_local_secret: vi.fn<WasmApplicationManager['show_local_secret']>().mockResolvedValue('private marker'),
    show_recovered: vi.fn<WasmApplicationManager['show_recovered']>().mockResolvedValue('recovered marker'),
    sign_up: vi.fn<WasmApplicationManager['sign_up']>().mockResolvedValue(stateFixture().state),
    state_events_auth_token: vi
      .fn<WasmApplicationManager['state_events_auth_token']>()
      .mockResolvedValue('state-token'),
    sync_now: vi.fn<WasmApplicationManager['sync_now']>().mockResolvedValue(),
    update_membership: vi.fn<WasmApplicationManager['update_membership']>().mockResolvedValue(),
  };
  return manager;
}
export function contextFixture(json = JSON.stringify(coreState())) {
  const { member, vaultData } = memberFixture(json);
  const manager = managerFixture();
  return { identity: 'session-a', epoch: 1, ready: true, member, manager, vaultData };
}
export const platformFixture = () => ({
  authenticate: vi.fn(async () => true),
  copy: vi.fn<(text: string) => Promise<void>>().mockResolvedValue(),
  readClipboard: vi.fn(async () => ''),
  reportError: vi.fn(),
});
