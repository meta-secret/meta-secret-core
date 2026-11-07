import type { LiveboardSession } from './types';

export const usable = (session: LiveboardSession | null) => Boolean(session?.member && session.ready);
export const sameSession = (left: LiveboardSession | null, right: LiveboardSession) =>
  left?.identity === right.identity && left.vaultId === right.vaultId && left.member === right.member;
export const cancelled = (error: unknown) => error instanceof DOMException && error.name === 'NotAllowedError';

/** Invalidates late platform promises without retaining sensitive operation values. */
export function createLifetime() {
  let revision = 0;
  let disposed = false;
  return {
    capture: () => revision,
    current: (token: number) => !disposed && token === revision,
    invalidate: () => revision++,
    dispose: () => {
      disposed = true;
      revision++;
    },
    get disposed() {
      return disposed;
    },
  };
}
