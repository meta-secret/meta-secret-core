# Apple callback infrastructure preparation

These commands prepare configuration; they do not create Cloudflare resources,
open a tunnel, change Apple Developer, push images, or deploy Kubernetes. The
existing broker remains optional and requires one server instance (RAM results,
300-second TTL; restart loses pending attempts).

## Public profiles and private runtime

On this host the verified tools and ignored public templates are already copied;
fill the real Apple Services ID associated with the correct Apple team/application.
For a new checkout, prepare templates without replacing an existing profile:

```bash
mkdir -p infra/.apple
chmod 700 infra/.apple
cp -n infra/apple/local.env.example infra/.apple/local.env
cp -n infra/apple/production.env.example infra/.apple/production.env
chmod 600 infra/.apple/*.env
```

Templates are intentionally incomplete. Blank Services ID plus callback/origins
fails as partial configuration. With no explicit profile, commands default to
disabled; disabled server exports contain **none** of the three Apple env names.
All three nonempty values are required to enable the broker. Files are parsed as
bounded data, never sourced/evaluated; unknown/duplicate keys, substitutions,
wrong domains/origins and placeholder IDs are rejected.

| Profile | Callback | Exact Web origins |
|---|---|---|
| local | `https://apple-dev.meta-secret.org/apple/callback` | `http://localhost:5173,http://localhost:5174` |
| production | `https://api.meta-secret.org/apple/callback` | `https://meta-secret.github.io` |

Only `APPLE_EMAIL_SERVICE_ID`, `APPLE_EMAIL_CALLBACK_URL` and
`APPLE_EMAIL_WEB_ORIGINS` are accepted. No Apple private key or Cloudflare token
belongs in these files. `infra/.apple/` is Git-ignored and excluded from the
taskomatic Docker context, including private credentials/runtime/tools.

```bash
task apple-config-check                          # disabled; no cloud activity
APPLE_INFRA_PROFILE=local task apple-config-check
APPLE_INFRA_PROFILE=local task apple-prepare
APPLE_INFRA_PROFILE=production task apple-config-check
APPLE_INFRA_PROFILE=production task apple-prepare
```

An explicit file can be selected with `APPLE_INFRA_ENV_FILE=/absolute/public.env`.
Profile names and input values are data; Task commands do not interpolate them
into shell code. Prepared outputs are private fixed files under
`infra/.apple/<profile>/`: `server.env`, `web.env`, `android.properties`. The Web
export contains only `VITE_APPLE_EMAIL_BROKER_URL`; the Android export contains
only `APPLE_EMAIL_BROKER_URL`. They are not installed into either app automatically.

**An Apple profile does not select Core Local.** Current application/Core
selection is independent and remains unchanged. Before real local Create/Join,
use the existing `task set-env -- local` workflow and its required native/WASM
artifact regeneration/rebuild steps. Rebuild clients after changing public Apple
settings. Configuration preparation and broker smoke do not prove application
or Core UI readiness.

## Explicit local server and tunnel

The local server requires an existing current `meta-secret/target/debug/meta-server`.
No helper builds Rust automatically. A stale/missing binary fails clearly.

```bash
APPLE_INFRA_PROFILE=local task apple-local-server
```

This runs in private `infra/.apple/local/server/`, initializing a fresh owned
SQLite only from the checked-in `db/sqlite/migrations/*/up.sql`. It does not copy a
real DB/key or use the absent production seed. An ownership receipt records the
migration hashes. Valid repeated starts preserve the DB and key; unowned files,
missing/inconsistent provenance, populated DB without key or symlinks fail closed.
Do not manually replace owned files with another runtime. The smoke check below
provides the recorded proof of fresh background initialization, schema/count
metadata and HTTP readiness, beyond `/hi` alone. The normal command keeps the
server in the foreground; this does not claim Core/application/provider readiness.
The existing binary binds `0.0.0.0:3000`; the helper's probes use only
`127.0.0.1`. An occupied port is never resolved by killing another process.

Cloudflare zone access, a stable named tunnel UUID, its DNS route for
`apple-dev.meta-secret.org` and its private credentials are separate owner steps.
No helper performs login, tunnel/DNS creation or credential copying. Once those
exist, place the private JSON in
`infra/.apple/credentials/<real-tunnel-uuid>.json` with mode600 and prepare:

```bash
APPLE_INFRA_PROFILE=local APPLE_TUNNEL_ID=<real-tunnel-uuid> task apple-prepare
```

This writes `infra/.apple/local/cloudflared.yml` (JSON is valid YAML). Four
anchored ingress paths, `/apple/start`, `/apple/callback`, `/apple/take` and
`/apple/cancel`, forward to `http://127.0.0.1:3000` on the exact local hostname.
The final rule returns404 for every other host/path. Core APIs, SSE and `/hi` are
not published by this tunnel; no rewrites or TLS weakening are configured.

For this host, verified tools can be kept at the following ignored paths. Keep
the KCL bundle and provenance together; do not change `HOME` or install globally:

```bash
export CLOUDFLARED_BIN="$PWD/infra/.apple/tools/cloudflared"
export KCL_BIN="$PWD/infra/.apple/tools/kcl-0.13.1/kcl"
```

In a **second terminal**, only after real credentials/DNS/Apple callback setup,
the owner can explicitly start the connector:

```bash
"$CLOUDFLARED_BIN" --no-autoupdate tunnel \
  --config "$PWD/infra/.apple/local/cloudflared.yml" run <real-tunnel-uuid>
```

No check/prepare Task depends on that command. Stop the two foreground processes
with Ctrl-C when finished. Never use debug request/body logging for OAuth.

## Local verification

```bash
task apple-infra-test
task apple-infra-syntax
task apple-infra-coverage          # stdlib helper-only statement coverage;80% advisory
CLOUDFLARED_BIN="$CLOUDFLARED_BIN" KCL_BIN="$KCL_BIN" task apple-tunnel-check
task apple-local-smoke
APPLE_INFRA_PROFILE=production KCL_BIN="$KCL_BIN" task apple-render
```

The real offline Cloudflared check validates generated syntax and14 actual Go
ingress matches, including denied Core/suffix/foreign-host/traversal paths. It
does not run a connector or read private credentials. KCL renders current full
YAML and the same StatefulSet JSON for disabled/configured cases, verifies one
replica and exactly three enabled env scalars, and rejects partial options.
The helper redirects documented KCL/package/XDG caches into ignored runtime;
KCL0.13.1's native Go loader on macOS may still need its standard compiler cache
under `~/Library/Caches/kcl`. This is compiler extraction, not cloud login.

`apple-local-smoke` uses a separate fresh owned fixture under `.apple/smoke/`,
not normal local runtime. It checks two real existing-binary starts, new per-child
background markers, migrated SQLite, `/hi`, synthetic broker start/cancel/take
and replay410, preserving owned DB count/key-file metadata across the restart.
It follows no authorization URL and makes no Apple/JWKS call. HTTP/startup and
shutdown use a30s aggregate deadline with at most0.2s forced-reap grace. Only its
own child is stopped, never a PID selected by port/name. Port conflicts or missing
binary are failures, not OAuth PASS. Root UI E2E/accounts/devices are not run.

Changed taskomatic image verification is separate:

```bash
task taskomatic                    # local image build only; no push/deploy
task apple-taskomatic-offline       # fresh image, network none; no kube/socket mounts
```

The bounded taskomatic build attempt and actual receipts are reported separately
from native compiler checks; native KCL success does not claim an unbuilt image.

## Future production deployment boundary

The StatefulSet remains one replica. K8s generation validates the exact public
process env before KCL rendering and before existing `kubectl apply`. Disabled
delivery omits every Apple env name. Taskomatic explicitly contains Python and
current helper/KCL/Taskfiles; runtime/credentials are excluded from its context.

From the workspace root, safe planning is available now:

```bash
scripts/deploy-server.sh --dry-run codex/orbit-registration
scripts/deploy-server.sh --dry-run \
  --apple-env meta-secret-core/infra/.apple/production.env codex/orbit-registration
```

Dry-run validates inputs without SSH/build/push/apply/rollout. The future actual
command retains the existing deployment behavior and requires separate owner
deployment authorization. Its static remote program rejects dirty tracked or
untracked nonignored files, pulls only the selected branch with fast-forward,
requires `HEAD == origin/<selected-branch>`, then validates public env, builds/
pushes the server through Task, rebuilds taskomatic and only then uses it. No
reset or stale-image fallback is performed; ignored `.apple` state is allowed.

**Configuration-only commits do not include the existing uncommitted broker/
Orbit/provider application WIP.** Before any real production deployment, that
reviewed feature code and the required matching server image must first be
committed/released on the selected remote Core branch. Preparing or committing
these infrastructure files is not a provider release, push, PR, merge or deploy.
See the owner's [activation plan](../../UxRevolution/implementation/apple-activation.md)
and the unchanged [provider contract](../docs/apple-email-provider.md).
