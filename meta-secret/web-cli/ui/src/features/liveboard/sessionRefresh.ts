export type SessionRefreshRequest = { requiredSync: boolean; target: number; revision: number };
type Flight = SessionRefreshRequest & { promise: Promise<void> };

/** Coalesces parent callbacks; the store remains the sole physical sync/read lane. */
export function createSessionRefresh(input: {
  active(): boolean;
  revision(): number;
  target(): number;
  require(): void;
  run(request: SessionRefreshRequest): Promise<void>;
}) {
  let flight: Flight | null = null;
  let queued: Flight | null = null;
  function start(request: SessionRefreshRequest): Promise<void> {
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const owner: Flight = { ...request, promise };
    flight = owner;
    void input
      .run(request)
      .then(resolve, reject)
      .finally(() => {
        if (flight === owner) flight = null;
      });
    return promise;
  }
  function refresh({ requiredSync = false }: { requiredSync?: boolean } = {}): Promise<void> {
    if (!input.active()) return Promise.resolve();
    if (requiredSync) input.require();
    const request = { requiredSync, target: input.target(), revision: input.revision() };
    if (!flight || flight.revision !== request.revision) return start(request);
    if (!requiredSync || (flight.requiredSync && flight.target >= request.target)) return flight.promise;
    if (queued?.revision === request.revision) {
      queued.target = Math.max(queued.target, request.target);
      return queued.promise;
    }
    const previous = flight.promise;
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const owner: Flight = { ...request, promise };
    queued = owner;
    void previous
      .catch(() => undefined)
      .then(async () => {
        if (queued === owner) queued = null;
        if (!input.active() || input.revision() !== owner.revision) return;
        await start(owner);
      })
      .then(resolve, reject);
    return promise;
  }
  return { refresh };
}
