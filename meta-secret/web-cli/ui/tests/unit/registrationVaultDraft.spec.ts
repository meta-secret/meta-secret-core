import { hydrateVaultNameDraft } from '@/components/vault/auth/registrationVaultDraft';
import { ref } from 'vue';
import { describe, expect, it } from 'vitest';

describe('hydrateVaultNameDraft', () => {
  it('writes trimmed name and marks draft as submitted when store name is non-empty', () => {
    const vaultName = ref('');
    const hasSubmittedVaultName = ref(false);
    hydrateVaultNameDraft('  my-vault  ', vaultName, hasSubmittedVaultName);
    expect(vaultName.value).toBe('my-vault');
    expect(hasSubmittedVaultName.value).toBe(true);
  });

  it('does nothing when store name is empty or whitespace-only', () => {
    const vaultName = ref('keep');
    const hasSubmittedVaultName = ref(false);
    hydrateVaultNameDraft('', vaultName, hasSubmittedVaultName);
    expect(vaultName.value).toBe('keep');
    expect(hasSubmittedVaultName.value).toBe(false);

    hydrateVaultNameDraft('   \t  ', vaultName, hasSubmittedVaultName);
    expect(vaultName.value).toBe('keep');
    expect(hasSubmittedVaultName.value).toBe(false);
  });
});
