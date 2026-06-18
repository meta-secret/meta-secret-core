import type { Ref } from 'vue';

export function hydrateVaultNameDraft(
  nameFromStore: string,
  vaultName: Ref<string>,
  hasSubmittedVaultName: Ref<boolean>,
): void {
  const name = nameFromStore.trim();
  if (!name) return;
  vaultName.value = name;
  hasSubmittedVaultName.value = true;
}
