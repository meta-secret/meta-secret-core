import type { WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

export function getAppManager(): WasmApplicationManager {
  return AppState().appManager;
}

export function getMemberVaultData(state: WasmApplicationState) {
  return state.as_vault?.()?.as_member?.()?.vault_data?.();
}

export function getMemberVaultState(state: WasmApplicationState) {
  return state.as_vault?.()?.as_member?.();
}

export function getDeviceId(state: WasmApplicationState): string {
  return state.device_id().wasm_id_str();
}
