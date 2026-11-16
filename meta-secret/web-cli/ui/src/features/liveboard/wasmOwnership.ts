import { PlainPassInfo } from 'meta-secret-web-cli';
import type {
  ClaimId,
  DeviceId,
  MetaPasswordId,
  WasmApplicationState,
  WasmUserMemberFullInfo,
} from 'meta-secret-web-cli';
import { ownedScope } from './ownedScope';

type Selection = { member: WasmUserMemberFullInfo; id: string };
const unavailable = () => new Error('OperationUnavailable');

export async function withOwnedPass<T>(
  { member, id }: Selection,
  action: (value: MetaPasswordId) => Promise<T>,
): Promise<T> {
  const scope = ownedScope();
  try {
    const vault = scope.own(member.vault_data());
    const passes = vault.secrets().map(scope.own);
    const matches = passes.filter((pass) => pass.id_str() === id);
    if (!id || matches.length !== 1) throw unavailable();
    return await action(matches[0]!);
  } finally {
    scope.release();
  }
}
export async function withOwnedClaim<T>({ member, id }: Selection, action: (value: ClaimId) => Promise<T>): Promise<T> {
  const scope = ownedScope();
  try {
    if (!id || new TextEncoder().encode(id).length > 512 || typeof member.recovery_claim_by_id !== 'function')
      throw unavailable();
    const found = member.recovery_claim_by_id(id);
    if (!found) throw unavailable();
    const claim = scope.own(found);
    if (claim.wasm_id_str() !== id) throw unavailable();
    return await action(claim);
  } finally {
    scope.release();
  }
}
export async function withOwnedDevice<T>(
  { member, id }: Selection,
  action: (value: DeviceId) => Promise<T>,
): Promise<T> {
  const scope = ownedScope();
  try {
    const vault = scope.own(member.vault_data());
    const users = vault.users().map(scope.own);
    const matches: DeviceId[] = [];
    for (const user of users) {
      const data = scope.own(user.user_data());
      const device = scope.own(data.device);
      const deviceId = scope.own(device.device_id);
      if (user.is_member() && deviceId.wasm_id_str() === id) matches.push(deviceId);
    }
    if (!id || matches.length !== 1) throw unavailable();
    return await action(matches[0]!);
  } finally {
    scope.release();
  }
}
export async function withOwnedPlainPass<T>(
  { name, text }: { name: string; text: string },
  action: (value: PlainPassInfo) => Promise<T>,
): Promise<T> {
  const input = new PlainPassInfo(name, text);
  try {
    return await action(input);
  } finally {
    input.free();
  }
}
export async function readOwnedMember<T>(
  { state }: { state: WasmApplicationState },
  mapper: (member: WasmUserMemberFullInfo) => T | Promise<T>,
): Promise<T> {
  const scope = ownedScope();
  try {
    if (!state.is_vault()) throw unavailable();
    const vault = scope.own(state.as_vault());
    if (!vault.is_member()) throw unavailable();
    const member = scope.own(vault.as_member());
    return await mapper(member);
  } finally {
    scope.release();
  }
}
