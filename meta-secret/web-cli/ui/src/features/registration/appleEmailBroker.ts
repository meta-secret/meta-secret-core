import { parseVaultName } from './registrationState';

const idPattern = /^[A-Za-z0-9_-]{43}$/;
const closedMessage = 'Apple sign-in was closed.';
const failureMessage = 'Apple sign-in could not be verified.';
export function configuredAppleBroker(input?: string): string | undefined {
  try {
    const url = new URL(input?.trim() ?? '');
    if (
      url.protocol !== 'https:' ||
      url.username ||
      url.password ||
      url.search ||
      url.hash ||
      url.pathname !== '/' ||
      url.hostname === 'localhost' ||
      url.hostname.endsWith('.localhost') ||
      url.hostname.includes(':') ||
      /^\d+(\.\d+){3}$/.test(url.hostname)
    )
      return;
    return url.origin;
  } catch {
    return;
  }
}
const encoded = (bytes: Uint8Array) =>
  btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
function flatJson(text: string): Record<string, unknown> {
  // This wire format is a flat object of primitives, not a general JSON protocol.
  const value: unknown = JSON.parse(text);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(failureMessage);
  const string = '"(?:[^"\\\\\\u0000-\\u001f]|\\\\(?:["\\\\/bfnrt]|u[0-9a-fA-F]{4}))*"';
  const field = new RegExp(
    `\\s*(${string})\\s*:\\s*(${string}|-?(?:0|[1-9]\\d*)(?:\\.\\d+)?(?:[eE][+-]?\\d+)?|true|false|null)\\s*`,
    'y',
  );
  const source = text.trim();
  let cursor = 1;
  const keys = new Set<string>();
  if (source === '{}') return value as Record<string, unknown>;
  while (cursor < source.length - 1) {
    field.lastIndex = cursor;
    const match = field.exec(source);
    if (!match) throw new Error(failureMessage);
    const key = JSON.parse(match[1]) as string;
    if (keys.has(key)) throw new Error(failureMessage);
    keys.add(key);
    cursor = field.lastIndex;
    if (source[cursor] === '}') break;
    if (source[cursor++] !== ',') throw new Error(failureMessage);
  }
  if (cursor !== source.length - 1) throw new Error(failureMessage);
  return value as Record<string, unknown>;
}
function exactKeys(value: Record<string, unknown>, keys: string[]) {
  if (Object.keys(value).sort().join(',') !== keys.sort().join(',')) throw new Error(failureMessage);
}
async function post(
  base: string,
  path: string,
  payload: unknown,
  signal?: AbortSignal,
): Promise<Record<string, unknown>> {
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener('abort', abort, { once: true });
  const deadline = setTimeout(abort, 30_000);
  if (signal?.aborted) abort();
  try {
    const response = await fetch(base + path, {
      method: 'POST',
      body: JSON.stringify(payload),
      headers: { 'Content-Type': 'application/json' },
      redirect: 'error',
      credentials: 'omit',
      cache: 'no-store',
      referrerPolicy: 'no-referrer',
      signal: controller.signal,
    });
    if (!response.ok || response.redirected || !response.body) throw new Error(failureMessage);
    const reader = response.body.getReader();
    let size = 0;
    const chunks: Uint8Array[] = [];
    try {
      while (true) {
        const result = await reader.read();
        if (result.done) break;
        size += result.value.byteLength;
        if (size > 16 * 1024) {
          await reader.cancel();
          throw new Error(failureMessage);
        }
        chunks.push(result.value);
      }
    } finally {
      reader.releaseLock();
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    return flatJson(new TextDecoder().decode(bytes));
  } finally {
    clearTimeout(deadline);
    signal?.removeEventListener('abort', abort);
  }
}
function authorization(input: unknown, base: string): string {
  if (typeof input !== 'string') throw new Error(failureMessage);
  const url = new URL(input);
  const params = url.searchParams;
  const keys = [...params.keys()];
  if (
    keys.length !== new Set(keys).size ||
    keys.sort().join(',') !== 'client_id,nonce,redirect_uri,response_mode,response_type,scope,state'
  )
    throw new Error(failureMessage);
  if (
    url.origin !== 'https://appleid.apple.com' ||
    url.pathname !== '/auth/authorize' ||
    url.hash ||
    url.username ||
    url.password ||
    params.get('redirect_uri') !== base + '/apple/callback' ||
    params.get('scope') !== 'email' ||
    params.get('response_mode') !== 'form_post' ||
    params.get('response_type') !== 'code id_token' ||
    !params.get('client_id') ||
    !idPattern.test(params.get('state') ?? '') ||
    !idPattern.test(params.get('nonce') ?? '')
  )
    throw new Error(failureMessage);
  return url.href;
}
export async function selectAppleBrokerEmail(input: string, signal?: AbortSignal): Promise<string> {
  const base = configuredAppleBroker(input);
  if (!base) throw new Error('Apple sign-in is not configured.');
  if (signal?.aborted) throw new Error(closedMessage);
  // Open within the button's user activation, before the asynchronous start.
  const popup = window.open('about:blank', '_blank', 'popup,width=520,height=680');
  if (!popup) throw new Error('Allow the Apple sign-in popup.');
  let verifier = encoded(crypto.getRandomValues(new Uint8Array(32)));
  let requestId: string | undefined;
  let success = false;
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener('abort', abort, { once: true });
  let cleanupWait = () => {};
  try {
    const challenge = encoded(
      new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))),
    );
    const started = await post(base, '/apple/start', { platform: 'web', challenge }, controller.signal);
    exactKeys(started, ['requestId', 'authorizationUrl', 'expiresAt']);
    if (
      typeof started.requestId !== 'string' ||
      !idPattern.test(started.requestId) ||
      typeof started.expiresAt !== 'number' ||
      !Number.isInteger(started.expiresAt) ||
      started.expiresAt * 1000 <= Date.now() ||
      started.expiresAt * 1000 > Date.now() + 330_000
    )
      throw new Error(failureMessage);
    requestId = started.requestId;
    const url = authorization(started.authorizationUrl, base);
    const returnedVia = await new Promise<'message' | 'closed'>((resolve, reject) => {
      const onMessage = (event: MessageEvent) => {
        if (
          event.origin === base &&
          event.source === popup &&
          event.data?.type === 'APPLE_EMAIL_RESULT' &&
          event.data?.requestId === requestId
        ) {
          cleanupWait();
          resolve('message');
        }
      };
      const onAbort = () => {
        cleanupWait();
        reject(new Error(closedMessage));
      };
      // Window liveness only; never poll provider, server, Core or Vault state.
      const windowWatch = setInterval(() => {
        if (popup.closed) {
          cleanupWait();
          resolve('closed');
        }
      }, 250);
      const deadline = setTimeout(onAbort, Math.min(300_000, (started.expiresAt as number) * 1000 - Date.now()));
      cleanupWait = () => {
        clearInterval(windowWatch);
        clearTimeout(deadline);
        window.removeEventListener('message', onMessage);
        controller.signal.removeEventListener('abort', onAbort);
      };
      window.addEventListener('message', onMessage);
      controller.signal.addEventListener('abort', onAbort, { once: true });
      if (controller.signal.aborted) onAbort();
      else popup.location.href = url;
    });
    const result = await post(base, '/apple/take', { requestId, verifier }, controller.signal);
    if (controller.signal.aborted) throw new Error(closedMessage);
    exactKeys(
      result,
      result.status === 'success'
        ? ['status', 'email']
        : result.status === 'error' && 'code' in result
          ? ['status', 'code']
          : ['status'],
    );
    if (result.status === 'cancelled' || (result.status === 'pending' && returnedVia === 'closed'))
      throw new Error(closedMessage);
    if (result.status !== 'success' || typeof result.email !== 'string') throw new Error(failureMessage);
    const email = parseVaultName(result.email);
    success = true;
    return email;
  } catch (error) {
    if (controller.signal.aborted || (error instanceof Error && error.message === closedMessage))
      throw new Error(closedMessage);
    throw new Error(failureMessage);
  } finally {
    cleanupWait();
    signal?.removeEventListener('abort', abort);
    controller.abort();
    if (!success && requestId) void post(base, '/apple/cancel', { requestId, verifier }).catch(() => undefined);
    verifier = '';
    popup.close();
  }
}
