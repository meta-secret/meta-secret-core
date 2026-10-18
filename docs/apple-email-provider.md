# Apple email selection transport

This optional module supplies a suggested Vault name. It does not create an
Apple account session, authenticate a Vault member or change Create/Join/Core
approval. Google and native iOS Apple remain independent. Actual account tests
are owner-manual; synthetic tests are not real provider acceptance.

## Activation boundary

No live HTTPS host or Services ID is assumed configured. Without all three
server settings the broker is disabled; partially supplied/invalid configuration
fails closed. Actual server deployment, DNS/TLS and correct-team Apple changes
require their separate authorization. A static GitHub Pages site cannot receive
Apple's required form POST; it can remain the Web frontend.

| Server public setting | Required value |
|---|---|
| APPLE_EMAIL_SERVICE_ID | Services ID associated with the correct team's primary Apple app |
| APPLE_EMAIL_CALLBACK_URL | Confirmed server HTTPS URL ending exactly /apple/callback, registered with Apple |
| APPLE_EMAIL_WEB_ORIGINS | Comma-separated exact allowed Web origins; explicit http://localhost:5174 is supported for local frontend checks |

Web public setting `VITE_APPLE_EMAIL_BROKER_URL` and Android
`APPLE_EMAIL_BROKER_URL` use that same confirmed HTTPS server origin. Rebuild
clients after changing public configuration. No private Apple key, client secret
or refresh token belongs in these settings. An unconfigured Apple action stays
unavailable. Placeholder test domains are not activation evidence.

## Wire contract

All request IDs/verifiers/challenges are 43-character unpadded base64url strings.
Verifier is 32 random bytes encoded to a string. Challenge hashes the **ASCII/UTF8
verifier string**, then base64url-encodes SHA256. It does not hash raw random bytes.

| POST path | Request / result |
|---|---|
| /apple/start | `{platform:"web"|"android",challenge}` → 200 `{requestId,authorizationUrl,expiresAt}`; expiresAt integer Unix seconds, TTL 300s |
| /apple/callback | Apple's application/x-www-form-urlencoded state/code/id_token or cancellation; never client-chosen return/JWKS/client ID |
| /apple/take | `{requestId,verifier}` → 200 `{status:"pending"}`, `{status:"success",email}`, `{status:"cancelled"}` or `{status:"error",code}`; terminal result consumed once |
| /apple/cancel | Same proof → 200 `{status:"cancelled"}`; discards any unconsumed email, fences in-flight verification and remains idempotent while retained |

Unknown/expired/consumed result 410; wrong proof 403; malformed 400,
content-type 415, oversized 413, capacity pressure 429. Only generic safe errors
are exposed. No HTTP redirects are followed by clients/JWKS fetch. Pending take
does not consume. Cancellation after take cannot resurrect a consumed result.

The server chooses random state/nonce and fixes Apple authorize/JWKS hosts,
Services ID, HTTPS callback and return targets. State lookup occurs before
network/signature work. A maintained JWT library verifies RS256 signature,
issuer/audience/nonce/sub/exp/iat/nbf/email_verified and code c_hash. Only signed
email is returned, with case/relay preserved. Raw user.email is ignored.
Code/id_token are discarded; no code exchange/account session/refresh lifecycle
is claimed. RAM retention is bounded to 1000 attempts / 300s; restarts lose state.
Use one server instance; multiple independent replicas do not share attempts.

Android returns to exactly
`metasecret://apple-auth/callback?requestId=<opaque-id>`. The Intent only wakes
the active adapter; it cannot supply email. Web receives only
`{type:"APPLE_EMAIL_RESULT",requestId}` from the broker popup and verifies the
exact Origin, Window reference and active ID before proof-bound take. Closing,
aborting or unmounting clears the client's attempt. No verifier/token/code/email
is placed in return URLs, persistent browser storage or logs.

## Operation and verification

Terminate HTTPS at the confirmed server ingress. Keep request bodies, headers
and callback contents out of ingress/application logs; no-store/no-referrer and
nonce CSP protect the completion page. Broker routes have their own strict CORS
and body limits, outside broad Core CORS/tracing layers. Public JWKS fetch is
fixed-host, no redirects, 5s, 256KiB, cached with bounded unknown-key refresh.

Local synthetic HTTP/adapter tests cover signatures, nonce/state, replay,
proofs, cancellation, concurrency, limits and exact browser return boundaries.
No production account or provider authorization is simulated as acceptance.
See the owner's [manual checks](../../UxRevolution/implementation/apple-manual-checks.md).
