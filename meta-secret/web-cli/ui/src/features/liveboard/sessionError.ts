type ErrorOwner = { showError(message: string): void; dismissError(): void };
type Context = readonly [ErrorOwner | undefined, unknown];

/** Metadata-only bridge; the parent coordinator remains the sole read owner. */
export function createSessionError(input: {
  context(): Context | null;
  active(): boolean;
  revoke(): void;
  require(): void;
  read(): Promise<void>;
}) {
  function capture(owner?: ErrorOwner) {
    const context = input.context();
    return context?.[0] && (!owner || context[0] === owner) ? context : null;
  }
  function current(context: Context) {
    const next = input.context();
    return next?.[0] === context[0] && next?.[1] === context[1];
  }
  function invalidate(owner: ErrorOwner) {
    const context = capture(owner);
    if (!context) return;
    input.revoke();
    if (current(context)) input.require();
  }
  return {
    bind<T extends ErrorOwner>(factory: (onError: () => void) => T): T {
      const binding: { owner?: T } = {};
      const created = factory(() => {
        if (binding.owner) invalidate(binding.owner);
      });
      binding.owner = created;
      return created;
    },
    report(message: string, output: (message: string) => void) {
      const context = capture();
      if (!context) return;
      context[0]!.showError(message);
      if (current(context)) output(message);
    },
    dismiss(clearDisplay: () => void) {
      const context = capture();
      if (!context) return;
      clearDisplay();
      if (!current(context)) return;
      context[0]!.dismissError();
      if (current(context) && input.active()) void input.read().catch(() => undefined);
    },
  };
}
