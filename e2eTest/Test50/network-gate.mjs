import { createServer, request as upstreamRequest } from 'node:http';
import { canonicalDeletionMetadata } from './metadata.mjs';

/** Test50 fault injection: sever existing SSE too; retain bounded held requests. */
export async function createDeletionNetworkGate({ port, upstreamPort = 3000, onEvent = () => {} }) {
  let mode = 'online';
  const sockets = new Set();
  const held = new Set();
  const heldWaiters = new Set();
  let metadata;
  let metadataFailure;
  const counters = { requests: 0, syncRequests: 0, syncResponses: 0, canonicalSnapshots: 0, statuses: {} };
  const forward = (request, response) => {
    const upstream = upstreamRequest({ hostname: '127.0.0.1', port: upstreamPort,
      method: request.method, path: request.url,
      headers: { ...request.headers, host: `127.0.0.1:${upstreamPort}` },
    }, (result) => {
      // Existing responses only: this observer never issues a synchronization request.
      if (request.url === '/meta_request') {
        counters.syncResponses += 1;
        const status = String(result.statusCode ?? 0); counters.statuses[status] = (counters.statuses[status] ?? 0) + 1;
        let size = 0; const chunks = [];
        result.on('data', (chunk) => { size += chunk.length; if (size <= 4 * 1024 * 1024) chunks.push(chunk); });
        result.on('end', () => {
          if (size > 4 * 1024 * 1024) { metadataFailure = new Error('Canonical metadata response exceeds the diagnostic bound'); return; }
          try { const observed = canonicalDeletionMetadata(JSON.parse(Buffer.concat(chunks).toString())); if (observed) counters.canonicalSnapshots += 1; if (observed && (!metadata || observed.revision > metadata.revision)) metadata = observed; }
          catch { metadataFailure = new Error('Canonical metadata response cannot be decoded (wire body omitted)'); }
        });
      }
      response.writeHead(result.statusCode ?? 502, result.headers); result.pipe(response);
    });
    upstream.on('error', () => { if (!response.headersSent) response.writeHead(502); response.end(); });
    response.on('close', () => upstream.destroy());
    request.pipe(upstream);
  };
  const server = createServer((request, response) => {
    counters.requests += 1; if (request.url === '/meta_request') counters.syncRequests += 1;
    if (mode === 'offline') { request.destroy(); return; }
    if (mode === 'hold') {
      const item = { request, response };
      held.add(item);
      for (const resolve of heldWaiters) resolve();
      heldWaiters.clear();
      response.on('close', () => held.delete(item));
      onEvent('request-held');
      return;
    }
    forward(request, response);
  });
  server.on('connection', (socket) => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, '0.0.0.0', resolve); });
  return {
    address: server.address(),
    counters() { return structuredClone(counters); },
    metadata() { if (metadataFailure) throw metadataFailure; if (!metadata) throw new Error(`Native canonical metadata missing; syncRequests=${counters.syncRequests} syncResponses=${counters.syncResponses} candidates=${counters.canonicalSnapshots} statuses=${JSON.stringify(counters.statuses)}`); return structuredClone(metadata); },
    block() { mode = 'offline'; for (const socket of sockets) socket.destroy(); onEvent('offline'); },
    hold() { mode = 'hold'; onEvent('hold'); },
    waitForHeld(timeoutMs = 30_000) {
      if (held.size > 0) return Promise.resolve();
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { heldWaiters.delete(ready); reject(new Error('Startup canonical HTTP request did not reach the hold gate')); }, timeoutMs);
        const ready = () => { clearTimeout(timer); resolve(); };
        heldWaiters.add(ready);
      });
    },
    unblock() { mode = 'online'; for (const item of held) forward(item.request, item.response); held.clear(); onEvent('online'); },
    async close() { for (const socket of sockets) socket.destroy(); await new Promise((resolve) => server.close(resolve)); },
  };
}
