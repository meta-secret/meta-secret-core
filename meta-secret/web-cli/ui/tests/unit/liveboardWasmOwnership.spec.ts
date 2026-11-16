import { describe, expect, it, vi } from 'vitest';
import type { ClaimId, DeviceId, MetaPasswordId, PlainPassInfo } from 'meta-secret-web-cli';
import {
  readOwnedMember,
  withOwnedClaim,
  withOwnedDevice,
  withOwnedPass,
  withOwnedPlainPass,
} from '@/features/liveboard/wasmOwnership';
import {
  claim,
  deferred,
  device,
  managerFixture,
  memberFixture,
  pass as fixturePass,
  stateFixture,
} from './liveboardCoreFixtures';

vi.mock('meta-secret-web-cli', () => {
  const lifetime = () => ({ free: vi.fn(), [Symbol.dispose]: vi.fn() });
  return {
    PlainPassInfo: class {
      free = vi.fn();
      [Symbol.dispose] = vi.fn();
      pass_id: MetaPasswordId;
      constructor(
        name: string,
        public pass: string,
      ) {
        this.pass_id = {
          ...lifetime(),
          id_str: vi.fn(() => 'pass-a'),
          name,
          id: { ...lifetime(), text: lifetime() },
        };
      }
    },
  };
});

describe('Liveboard owned generated wrappers across borrowed awaits', () => {
  it('retains PlainPassInfo constructor input until checked Create settles and frees it once', async () => {
    const manager = managerFixture();
    const created = deferred<string>();
    vi.mocked(manager.cluster_distribution_checked).mockReturnValue(created.promise);
    let captured: PlainPassInfo | undefined;
    const pending = withOwnedPlainPass(
      { name: 'Same name', text: 'transient private input' },
      async (input: PlainPassInfo) => {
        captured = input;
        return manager.cluster_distribution_checked(input);
      },
    );
    expect(captured?.pass).toBe('transient private input');
    expect(captured?.pass_id.name).toBe('Same name');
    expect(captured?.free).not.toHaveBeenCalled();
    created.resolve('bare checked receipt');
    await pending;
    expect(manager.cluster_distribution_checked).toHaveBeenCalledExactlyOnceWith(captured);
    expect(captured?.free).toHaveBeenCalledOnce();
    expect(manager.cluster_distribution).not.toHaveBeenCalled();
  });
  it.each(['resolve', 'reject'] as const)(
    'keeps SAME owned SECOND Claim through parent.free and Show %s, then frees once',
    async (outcome) => {
      const { member } = memberFixture();
      const first = claim('claim-first');
      const second = claim('claim-second');
      vi.mocked(member.recovery_claim_by_id).mockImplementation((id) =>
        id === 'claim-first' ? first : id === 'claim-second' ? second : undefined,
      );
      const manager = managerFixture();
      const show = deferred<string>();
      vi.mocked(manager.show_recovered).mockReturnValue(show.promise);
      const pending = withOwnedClaim({ member, id: 'claim-second' }, async (owned: ClaimId) => {
        expect(owned).toBe(second);
        const result = manager.show_recovered(owned);
        member.free(); // Simulates an independent owned snapshot going away, not a store-root disposal.
        return result;
      });
      expect(manager.show_recovered).toHaveBeenCalledExactlyOnceWith(second);
      expect(second.free).not.toHaveBeenCalled();
      expect(first.free).not.toHaveBeenCalled();
      if (outcome === 'reject') {
        show.reject(new Error('sanitized test rejection'));
        await expect(pending).rejects.toThrow();
      } else {
        show.resolve('private result');
        await expect(pending).resolves.toBe('private result');
      }
      expect(second.free).toHaveBeenCalledOnce();
      expect(member.recovery_claim_by_id).toHaveBeenCalledExactlyOnceWith('claim-second');
      expect(member.find_recovery_claim).not.toHaveBeenCalled();
      expect(member.find_pending_incoming_recovery_claim).not.toHaveBeenCalled();
    },
  );

  it('rejects an unresolved exact Claim instead of selecting the first same-Pass Claim', async () => {
    const { member } = memberFixture();
    const action = vi.fn();
    await expect(withOwnedClaim({ member, id: 'missing-exact-claim' }, action)).rejects.toThrow();
    expect(action).not.toHaveBeenCalled();
    expect(member.free).not.toHaveBeenCalled();
    expect(member.find_pending_incoming_recovery_claim).not.toHaveBeenCalled();
  });

  it.each(['resolve', 'reject'] as const)(
    'keeps exact Pass borrowed during Test50-style cancellation, %s then free once',
    async (outcome) => {
      const { member, vaultData } = memberFixture();
      const exact = fixturePass('pass-a');
      const other = fixturePass('pass-b');
      vi.mocked(vaultData.secrets).mockReturnValue([other, exact]);
      const manager = managerFixture();
      const show = deferred<string>();
      vi.mocked(manager.show_local_secret).mockReturnValue(show.promise);
      let visible = true;
      const pending = withOwnedPass({ member, id: 'pass-a' }, async (owned: MetaPasswordId) => {
        expect(owned).toBe(exact);
        const text = await manager.show_local_secret(owned);
        return visible ? text : null;
      });
      visible = false;
      expect(exact.free).not.toHaveBeenCalled();
      expect(manager.show_local_secret).toHaveBeenCalledExactlyOnceWith(exact);
      if (outcome === 'reject') {
        show.reject(new Error('failed Show'));
        await expect(pending).rejects.toThrow();
      } else {
        show.resolve('late private value');
        await expect(pending).resolves.toBeNull();
      }
      expect(exact.free).toHaveBeenCalledOnce();
      expect(other.free).toHaveBeenCalledOnce();
      expect(vaultData.free).toHaveBeenCalledOnce();
      expect(member.free).not.toHaveBeenCalled();
      expect(manager.free).not.toHaveBeenCalled();
    },
  );

  it('borrows the selected Device for checked Remove without consuming/reusing it or freeing a root', async () => {
    const { member, vaultData } = memberFixture();
    const users = vaultData.users();
    const target = device('peer-a');
    const targetData = users[1]!.user_data().device;
    targetData.device_id = target;
    vi.mocked(vaultData.users).mockReturnValue(users);
    const members = vaultData.members();
    const memberTarget = device('peer-a');
    members[1]!.user_data.device.device_id = memberTarget;
    vi.mocked(vaultData.members).mockReturnValue(members);
    const manager = managerFixture();
    const removing = deferred<string>();
    vi.mocked(manager.remove_member_checked).mockReturnValue(removing.promise);
    const pending = withOwnedDevice({ member, id: 'peer-a' }, async (owned: DeviceId) =>
      manager.remove_member_checked(owned),
    );
    const borrowed = vi.mocked(manager.remove_member_checked).mock.calls[0]![0];
    expect(borrowed.wasm_id_str()).toBe('peer-a');
    const actualOwnedIds = [
      ...vi
        .mocked(vaultData.users)
        .mock.results.flatMap((result) =>
          result.type === 'return' ? result.value.map((user) => user.user_data().device.device_id) : [],
        ),
      ...vi
        .mocked(vaultData.members)
        .mock.results.flatMap((result) =>
          result.type === 'return' ? result.value.map((member) => member.user_data.device.device_id) : [],
        ),
    ];
    expect(actualOwnedIds).toContain(borrowed);
    expect(manager.remove_member_checked).toHaveBeenCalledExactlyOnceWith(borrowed);
    expect(borrowed.free).not.toHaveBeenCalled();
    removing.resolve('receipt');
    await pending;
    expect(borrowed.free).toHaveBeenCalledOnce();
    expect(member.free).not.toHaveBeenCalled();
    expect(manager.remove_member_from_vault).not.toHaveBeenCalled();
  });

  it.each(['resolve', 'throw'] as const)(
    'releases snapshot getter children in reverse order on %s and never store roots',
    async (outcome) => {
      const { state, vault, member } = stateFixture();
      const order: string[] = [];
      vi.mocked(member.free).mockImplementation(() => {
        order.push('member');
      });
      vi.mocked(vault.free).mockImplementation(() => {
        order.push('vault');
      });
      const mapping = deferred<string>();
      const pending = readOwnedMember({ state }, async (owned: typeof member) => {
        expect(owned).toBe(member);
        return mapping.promise;
      });
      expect(member.free).not.toHaveBeenCalled();
      if (outcome === 'throw') {
        mapping.reject(new Error('mapping failed'));
        await expect(pending).rejects.toThrow();
      } else {
        mapping.resolve('metadata only');
        await expect(pending).resolves.toBe('metadata only');
      }
      expect(order).toEqual(['member', 'vault']);
      expect(member.free).toHaveBeenCalledOnce();
      expect(vault.free).toHaveBeenCalledOnce();
      expect(state.free).not.toHaveBeenCalled();
    },
  );
});
