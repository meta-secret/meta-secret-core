export type RegistrationSnapshot = {
  kind: 'local' | 'available' | 'exists' | 'pending' | 'declined' | 'member';
  name: string;
};
export type RegistrationPhase =
  | 'choice'
  | 'checking'
  | 'available'
  | 'exists'
  | 'editing'
  | 'join'
  | 'creating'
  | 'requesting'
  | 'pending'
  | 'syncing'
  | 'error';
export type RegistrationState = {
  step: 1 | 2 | 3 | 4;
  name: string;
  phase: RegistrationPhase;
  message: string;
  direction: 1 | -1;
  ready: boolean;
  timedCreateError: boolean;
};
export function parseVaultName(input: string): string {
  const name = input.trim();
  if (
    !name ||
    name.length > 254 ||
    /[\x00-\x1f\x7f]/.test(name) ||
    /^(?:[a-z][a-z0-9+.-]*:|[\[{])/i.test(name) ||
    name.startsWith('//')
  ) {
    throw new Error('This QR code is not a Vault name.');
  }
  return name;
}
export function parseEmail(input: string): string {
  const name = input.trim();
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(name) || name.length > 254) throw new Error('Enter a valid email.');
  return name;
}
