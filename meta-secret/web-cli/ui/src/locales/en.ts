/**
 * UI strings (English). Keep user-visible copy here; do not hardcode in components.
 */
export const vaultSecrets = {
  title: 'Your Secrets',
  addSecret: '+ Add Secret',
  emptyState: 'No secrets added yet',
  show: 'Show',
  showLoading: 'Loading...',
  waitingTitle: 'Secret is encrypted',
  waitingSubtitle: "Requesting recovery shares to decrypt and reveal the content",
  waitingDevicesSuffix: 'device',
  waitingDevicesSuffixPlural: 'devices',
  close: 'Close',
  copySecret: 'Copy secret',
  copyPhrase: 'Copy phrase',
  secretLabel: 'Secret:',
  errorShowRecovered:
    'Could not display the recovered secret. Shares may be incomplete or corrupted.',
  errorCopySecret: 'Could not copy the secret.',
  errorRecoveryTimeout: 'Recovery took too long. Please try again.',
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
