import { shallowRef } from 'vue';
import type {
  WasmApplicationManager,
  WasmApplicationState,
  WasmUserMemberFullInfo,
  WasmVaultFullInfo,
} from 'meta-secret-web-cli';
import type { CoreContext } from './coreTypes';

type Lease = {
  root: WasmApplicationState;
  manager: WasmApplicationManager;
  generation: number;
  vault: WasmVaultFullInfo;
  member: WasmUserMemberFullInfo;
  vaultName: string;
  deviceId: string;
};

/** Fresh getter children belong to this lease; store manager/root handles never do. */
export function createSessionContext() {
  const context = shallowRef<CoreContext | null>(null);
  let lease: Lease | null = null;
  let identity = 0;
  let epoch = 0;
  function release(value: Lease | null) {
    value?.member.free();
    value?.vault.free();
  }
  function block() {
    epoch++;
    if (context.value) context.value = { ...context.value, epoch, ready: false };
  }
  function clear() {
    const previous = lease;
    lease = null;
    context.value = null;
    epoch++;
    release(previous);
  }
  function publish(input: {
    root: WasmApplicationState | null;
    manager: WasmApplicationManager | null;
    generation: number;
    ready: boolean;
  }) {
    const { root, manager, generation, ready } = input;
    if (!root || !manager || !root.is_vault()) {
      const changed = lease !== null;
      clear();
      return changed;
    }
    if (lease?.root === root && lease.manager === manager && lease.generation === generation) {
      if (context.value) context.value = { ...context.value, ready };
      return false;
    }
    const vault = root.as_vault();
    let member: WasmUserMemberFullInfo | null = null;
    try {
      if (!vault.is_member()) {
        const changed = lease !== null;
        clear();
        return changed;
      }
      member = vault.as_member();
      const device = root.device_id();
      let deviceId: string;
      try {
        deviceId = device.wasm_id_str();
      } finally {
        device.free();
      }
      const vaultName = vault.vault_name();
      if (!deviceId || !vaultName) throw new Error('OperationUnavailable');
      const changed =
        !lease ||
        lease.manager !== manager ||
        lease.generation !== generation ||
        lease.vaultName !== vaultName ||
        lease.deviceId !== deviceId;
      if (changed) identity++;
      epoch++;
      const previous = lease;
      lease = { root, manager, generation, vault, member, vaultName, deviceId };
      member = null;
      context.value = { identity: `vault-session:${identity}`, epoch, ready, manager, member: lease.member };
      release(previous);
      return changed;
    } finally {
      if (lease?.vault !== vault) {
        member?.free();
        vault.free();
      }
    }
  }
  return {
    context,
    block,
    clear,
    publish,
    vaultName: () => lease?.vaultName ?? '',
    manager: () => lease?.manager ?? null,
    root: () => lease?.root ?? null,
  };
}
