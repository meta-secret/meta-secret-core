import { describe, expect, it } from 'vitest';
import { shouldShowRecoveryRequestIcon } from '@/utils/recoveryRequest';

describe('shouldShowRecoveryRequestIcon', () => {
  it('shows the icon when an incoming recovery claim exists', () => {
    expect(shouldShowRecoveryRequestIcon({ claim: 'id' })).toBe(true);
  });

  it('hides the icon when there is no incoming recovery claim', () => {
    expect(shouldShowRecoveryRequestIcon(undefined)).toBe(false);
    expect(shouldShowRecoveryRequestIcon(null)).toBe(false);
  });
});
