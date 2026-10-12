import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, get } from 'node:http';
import { once } from 'node:events';
import { createDeletionNetworkGate } from './network-gate.mjs';

test('offline closes an existing event stream; hold releases startup response only at barrier', { timeout: 5000 }, async () => {
  const upstream = createServer((request, response) => {
    if (request.url === '/events') { response.writeHead(200, { 'content-type': 'text/event-stream' }); response.write('event: ready\ndata: {}\n\n'); }
    else response.end('canonical-ready');
  });
  upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
  let held;
  const holdObserved = new Promise((resolve) => { held = resolve; });
  const gate = await createDeletionNetworkGate({ port: 0, upstreamPort: upstream.address().port, onEvent: (event) => { if (event === 'request-held') held(); } });
  try {
    const stream = await new Promise((resolve, reject) => { const request = get(`http://127.0.0.1:${gate.address.port}/events`, resolve); request.on('error', reject); });
    stream.resume();
    const closed = new Promise((resolve, reject) => { stream.once('close', resolve); stream.on('error', (error) => { if (error.code !== 'ECONNRESET') reject(error); }); }); gate.block(); await closed;
    gate.hold();
    let completed = false;
    const result = new Promise((resolve, reject) => {
      const request = get(`http://127.0.0.1:${gate.address.port}/startup`, (response) => {
        let text = ''; response.on('data', (chunk) => { text += chunk; }); response.on('end', () => { completed = true; resolve(text); });
      }); request.on('error', reject);
    });
    await holdObserved; await gate.waitForHeld(); assert.equal(completed, false);
    gate.unblock(); assert.equal(await result, 'canonical-ready');
  } finally { await gate.close(); upstream.closeAllConnections(); await new Promise((resolve) => upstream.close(resolve)); }
});
