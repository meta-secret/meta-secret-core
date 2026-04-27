/**
 * UI strings (English). Keep user-visible copy here; do not hardcode in components.
 */
export const vaultSecrets = {
  errorShowRecovered:
    'Could not display the recovered secret. Shares may be incomplete or corrupted.',
  errorCopySecret: 'Could not copy the secret.',
} as const;

export const vaultDevices = {
  statusCurrent: 'Current',
  statusMember: 'Member',
  statusPending: 'Pending',
  statusDeclined: 'Declined',
  actionAccept: 'Accept',
  actionDecline: 'Decline',
  confirmJoinPrefix: 'Are you sure you want to join',
  confirmJoinMiddle: 'to your vault',
  fallbackVaultName: 'Vault',
} as const;
