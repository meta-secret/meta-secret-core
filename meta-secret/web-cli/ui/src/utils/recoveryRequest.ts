export function shouldShowRecoveryRequestIcon(claimId: unknown | undefined): boolean {
  return claimId !== undefined && claimId !== null;
}
