import { describe, expect, it } from 'vitest';
import { parseEmail, parseVaultName } from '@/features/registration/registrationState';

describe('QR selects a name only', () => {
  it.each(['owner@example.com', 'family-vault', 'Family Vault', 'q', 'relay@privaterelay.appleid.com'])(
    'accepts plain %s',
    (name) => {
      expect(parseVaultName(` ${name} `)).toBe(name);
    },
  );
  it.each(['https://evil.test/?token=x', 'javascript:alert(1)', 'name\nother', '', 'a'.repeat(255)])(
    'rejects non-name payload',
    (input) => {
      expect(() => parseVaultName(input)).toThrow('This QR code is not a Vault name.');
    },
  );
});

describe('Manual email', () => {
  it('preserves address case and accepts Apple relay', () => {
    expect(parseEmail(' Person@Example.com ')).toBe('Person@Example.com');
  });
  it('rejects a plain name in the email field', () => {
    expect(() => parseEmail('Family')).toThrow('Enter a valid email.');
  });
});
