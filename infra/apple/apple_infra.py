#!/usr/bin/env python3
"""Local Apple transport preparation. No cloud actions in check/prepare/render/smoke."""
import argparse
import base64
from contextlib import closing
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import socket
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]
INFRA = Path(__file__).resolve().parents[1]
RUNTIME = INFRA / ".apple"
FIELDS = ("APPLE_EMAIL_SERVICE_ID", "APPLE_EMAIL_CALLBACK_URL", "APPLE_EMAIL_WEB_ORIGINS")
HOSTS = {"local": "apple-dev.meta-secret.org", "production": "api.meta-secret.org"}
ORIGINS = {"local": "http://localhost:5173,http://localhost:5174", "production": "https://meta-secret.github.io"}
PATHS = ("/apple/start", "/apple/callback", "/apple/take", "/apple/cancel")
IMAGE = "ghcr.io/meta-secret/meta-secret-core/meta-secret-server:latest"


class ConfigError(Exception):
    """Payload-free actionable configuration failure."""


def parse_profile(profile, content):
    if profile not in HOSTS or len(content.encode("utf-8")) > 16384:
        raise ConfigError("Invalid profile or oversized public configuration")
    values = {}
    for raw in content.split("\n"):
        line = raw[:-1] if raw.endswith("\r") else raw
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, sep, value = line.partition("=")
        if not sep or key not in FIELDS or key in values or not re.fullmatch(r"[A-Za-z0-9_.,:/-]*", value):
            raise ConfigError("Malformed, duplicate or unknown public configuration field")
        values[key] = value
    if not any(values.values()):
        return {}
    if set(values) != set(FIELDS) or not all(values.values()):
        raise ConfigError("Apple configuration is partial; supply all three nonempty fields")
    service = values[FIELDS[0]]
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,254}", service) or service.startswith(("REPLACE", "YOUR_")):
        raise ConfigError("Invalid or placeholder Services ID")
    if values[FIELDS[1]] != f"https://{HOSTS[profile]}/apple/callback" or values[FIELDS[2]] != ORIGINS[profile]:
        raise ConfigError("Callback or exact origins do not match the selected profile")
    return values


def validate_values(profile, values):
    if not isinstance(values, dict) or any(not isinstance(k, str) or not isinstance(v, str) or "\n" in v or "\r" in v for k, v in values.items()):
        raise ConfigError("Invalid public configuration object")
    return parse_profile(profile, "\n".join(k + "=" + v for k, v in values.items()))


def no_symlinks(path):
    path = Path(path).absolute()
    for part in [*reversed(path.parents), path]:
        if part.is_symlink():
            raise ConfigError("Symlink configuration/runtime paths are not allowed")
    return path


def safe_dir(path):
    path = no_symlinks(path)
    for directory in [*reversed(path.parents), path]:
        if not directory.exists():
            directory.mkdir(mode=0o700)
    if path.is_relative_to(RUNTIME) and RUNTIME.is_dir():
        RUNTIME.chmod(0o700)
    if not path.is_dir():
        raise ConfigError("Runtime directory is not a directory")
    path.chmod(0o700)
    return path


def atomic_write(path, content):
    path = no_symlinks(path)
    safe_dir(path.parent)
    if path.exists() and not stat.S_ISREG(path.stat().st_mode):
        raise ConfigError("Runtime output is not a regular file")
    fd, temporary = tempfile.mkstemp(prefix=".prepare-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as output:
            output.write(content)
        no_symlinks(path)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read_profile(profile, path):
    path = no_symlinks(path)
    if not path.is_file() or path.stat().st_size > 16384:
        raise ConfigError("Public profile file missing or oversized")
    return parse_profile(profile, path.read_text(encoding="utf-8"))


def selected_profile():
    name = os.environ.get("APPLE_INFRA_PROFILE", "disabled")
    filename = os.environ.get("APPLE_INFRA_ENV_FILE")
    if name == "disabled":
        if filename:
            raise ConfigError("Choose local or production for an explicit profile file")
        return "local", {}
    if name not in HOSTS:
        raise ConfigError("Profile must be local, production or disabled")
    return name, read_profile(name, filename or RUNTIME / (name + ".env"))


def exports(profile, values):
    values = validate_values(profile, values)
    broker = "https://" + HOSTS[profile] if values else ""
    return {"server.env": "".join(k + "=" + values[k] + "\n" for k in FIELDS if k in values),
            "web.env": "VITE_APPLE_EMAIL_BROKER_URL=" + broker + "\n",
            "android.properties": "APPLE_EMAIL_BROKER_URL=" + broker + "\n"}


def tunnel_config(identifier):
    try:
        if str(uuid.UUID(identifier)) != identifier or uuid.UUID(identifier).int == 0:
            raise ValueError()
    except (ValueError, AttributeError):
        raise ConfigError("Tunnel ID must be a canonical nonzero UUID") from None
    return {"tunnel": identifier, "credentials-file": str(RUNTIME / "credentials" / (identifier + ".json")),
            "ingress": [{"hostname": HOSTS["local"], "path": "^" + path + "$", "service": "http://127.0.0.1:3000"} for path in PATHS] + [{"service": "http_status:404"}]}


def prepare(profile, values, identifier=None):
    data = exports(profile, values)
    if identifier:
        if profile != "local" or not values:
            raise ConfigError("A tunnel config requires a configured local profile")
        data["cloudflared.yml"] = json.dumps(tunnel_config(identifier), indent=2) + "\n"
        credential = no_symlinks(RUNTIME / "credentials" / (identifier + ".json"))
        if credential.parent.exists() and not credential.parent.is_dir():
            raise ConfigError("Credential directory is not a directory")
    for name in data:
        path = no_symlinks(RUNTIME / profile / name)
        if path.exists() and not path.is_file() or any(p.exists() and not p.is_dir() for p in path.parents):
            raise ConfigError("Prepared output destination is not a regular file/directory")
    for name, content in data.items():
        atomic_write(RUNTIME / profile / name, content)
    if identifier:
        safe_dir(RUNTIME / "credentials")
    return RUNTIME / profile


def validate_branch(branch):
    if branch == "HEAD" or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]{0,199}", branch):
        raise ConfigError("Invalid deployment branch")
    result = subprocess.run(["git", "check-ref-format", "--branch", branch], capture_output=True, timeout=5)
    if result.returncode:
        raise ConfigError("Invalid deployment branch")
    return branch


REMOTE_PROGRAM = r'''set -euo pipefail
cd meta/meta-secret-core
[[ "$1" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$ ]] && [[ "$1" != HEAD ]]
git check-ref-format --branch "$1" >/dev/null
test -z "$(git status --porcelain --untracked-files=normal)" || { echo "Remote checkout has local changes; refusing deployment" >&2; exit 1; }
git fetch origin
git checkout "origin/$1"
git switch "$1" || git switch -c "$1" --track "origin/$1"
git pull --ff-only origin "$1"
test "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$1")" || { echo "Remote checkout differs from selected origin branch" >&2; exit 1; }
test -z "$(git status --porcelain --untracked-files=normal)" || { echo "Remote checkout has local changes; refusing deployment" >&2; exit 1; }
printf '%s' "$2" | python3 infra/apple/apple_infra.py install-production
task meta-server-push
task taskomatic
mkdir -p "$HOME/.kube"
cp "$HOME/.kube/config" "$HOME/.kube/config.bak.$(date +%Y%m%d-%H%M%S)" 2>/dev/null || true
k3d kubeconfig get meta-secret > "$HOME/.kube/config"
chmod 600 "$HOME/.kube/config"
docker run --rm --network host -v /var/run/docker.sock:/var/run/docker.sock -v "$HOME/.kube:/root/.kube" -v "$HOME/.config/k3d:/root/.config/k3d" --env-file infra/.apple/production/server.env -e APPLE_INFRA_PROFILE=production --workdir /taskomatic localhost/taskomatic:latest k8s:deploy_meta_server
kubectl rollout restart statefulset/meta-secret-server
kubectl rollout status statefulset/meta-secret-server
'''


def remote_deploy(branch, values):
    validate_branch(branch)
    values = validate_values("production", values)
    payload = base64.urlsafe_b64encode(json.dumps(values, separators=(",", ":")).encode()).decode()
    return ["/usr/bin/ssh", "debian@188.165.236.156", "bash -s -- " + shlex.join([branch, payload])], REMOTE_PROGRAM


def deploy(arguments):
    parser = argparse.ArgumentParser(description="Future remote deployment; --dry-run never contacts the server")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--apple-env", type=Path)
    parser.add_argument("branch", nargs="?")
    args = parser.parse_args(arguments)
    values = read_profile("production", args.apple_env) if args.apple_env else {}
    branch = args.branch
    if not branch:
        branch = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--abbrev-ref", "HEAD"], text=True, timeout=5).strip()
    command, program = remote_deploy(branch, values)
    if args.dry_run:
        print(json.dumps({"mode": "dry-run", "branch": branch, "apple": "configured" if values else "disabled", "steps": ["validate public production profile", "git fetch/switch/pull", "task meta-server-push", "task taskomatic", "taskomatic k8s:deploy_meta_server", "kubectl rollout restart/status"]}, indent=2))
        return
    subprocess.run(command, input=program, text=True, check=True)


def install_production():
    encoded = sys.stdin.buffer.read(32769)
    if len(encoded) > 32768:
        raise ConfigError("Oversized public deployment payload")
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ConfigError("Duplicate deployment field")
            result[key] = value
        return result
    try:
        data = json.loads(base64.b64decode(encoded, altchars=b"-_", validate=True), object_pairs_hook=unique)
        values = validate_values("production", data)
    except (ValueError, UnicodeError, TypeError):
        raise ConfigError("Invalid public deployment payload") from None
    atomic_write(RUNTIME / "production/server.env", exports("production", values)["server.env"])


def kcl_command(values, module=False):
    values = validate_values("production", values)
    tool = os.environ.get("KCL_BIN", "kcl")
    source = INFRA / "k8s/meta-secret-server" / ("statefulset/statefulset.k" if module else "main.k")
    command = [tool, "run", str(source)]
    if module:
        command += ["--format", "json"]
    command += ["-D", "imageName=" + json.dumps(IMAGE)]
    for key, option in zip(FIELDS, ("appleEmailServiceId", "appleEmailCallbackUrl", "appleEmailWebOrigins")):
        command += ["-D", option + "=" + json.dumps(values.get(key, ""))]
    return command


def tool_environment():
    env = dict(os.environ)
    for name, directory in {"KCL_PKG_PATH": "kcl-packages", "KCL_CACHE_PATH": "kcl-native", "XDG_DATA_HOME": "data", "XDG_CACHE_HOME": "cache", "XDG_CONFIG_HOME": "config"}.items():
        env[name] = str(safe_dir(RUNTIME / "tools" / directory))
    return env


def render_manifest(values, module=False):
    command = kcl_command(values, module)
    result = subprocess.run(command, cwd=INFRA / "k8s", env=tool_environment(), capture_output=True, text=True, timeout=60)
    if result.returncode:
        print(result.stderr[:4096], file=sys.stderr)
        raise ConfigError("KCL render failed; inspect the local compiler receipt")
    if not result.stdout.strip() or len(result.stdout) > 1024 * 1024:
        raise ConfigError("KCL produced empty or oversized output")
    return result.stdout


def migration_sources():
    paths = sorted((ROOT / "meta-secret/db/sqlite/migrations").glob("*/up.sql"))
    if not paths:
        raise ConfigError("Repository SQLite migrations missing")
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}


def database_count(path):
    no_symlinks(path)
    try:
        with closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True, timeout=2)) as connection:
            columns = [row[1] for row in connection.execute("PRAGMA table_info(db_commit_log)")]
            if columns != ["id", "key_id", "event"]:
                raise ConfigError("Owned SQLite schema mismatch")
            return connection.execute("SELECT count(*) FROM db_commit_log").fetchone()[0]
    except sqlite3.Error:
        raise ConfigError("Owned SQLite metadata unavailable") from None


def initialize_runtime(path):
    path = no_symlinks(path)
    for name in ["meta-secret.db", "meta-secret.db-wal", "meta-secret.db-shm", "master_key.json", "owner.json", "server.log"]:
        item = no_symlinks(path / name)
        if item.exists() and not item.is_file():
            raise ConfigError("Owned runtime contains a non-file")
    expected = {"version": 1, "migrations": migration_sources()}
    receipt = path / "owner.json"; database = path / "meta-secret.db"; key = path / "master_key.json"
    if receipt.exists():
        try:
            owned = json.loads(receipt.read_text()) if receipt.stat().st_size <= 16384 else None
        except (ValueError, UnicodeError):
            owned = None
        if owned != expected or not database.is_file():
            raise ConfigError("Owned runtime provenance mismatch")
        if database_count(database) and not key.is_file():
            raise ConfigError("Populated owned runtime lost its key; refusing new identity")
        return path
    if path.exists() and any(path.iterdir()):
        raise ConfigError("Existing runtime is unowned; refusing to overwrite it")
    safe_dir(path)
    with closing(sqlite3.connect(database)) as connection:
        for name in expected["migrations"]:
            connection.executescript((ROOT / name).read_text())
    database.chmod(0o600)
    database_count(database)
    atomic_write(receipt, json.dumps(expected, indent=2) + "\n")
    return path


def server_binary():
    binary = ROOT / "meta-secret/target/debug/meta-server"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ConfigError("Existing debug meta-server binary unavailable; no automatic build")
    sources = [ROOT / "meta-secret/Cargo.toml"]
    for directory in ["core", "db/sqlite", "meta-server/server-node", "meta-server/web-server"]:
        sources += list((ROOT / "meta-secret" / directory).rglob("*.rs"))
        sources.append(ROOT / "meta-secret" / directory / "Cargo.toml")
    if any(source.stat().st_mtime_ns > binary.stat().st_mtime_ns for source in sources):
        raise ConfigError("Existing server binary predates Rust inputs; rebuild separately")
    return binary


def free_port():
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            listener.bind(("0.0.0.0", 3000))
            listener.listen(1)
        except OSError as error:
            if error.errno == errno.EADDRINUSE:
                raise ConfigError(f"Port3000 bind failed errno={error.errno} at {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}; no existing process will be stopped") from None
            raise ConfigError("Local port preflight unavailable; no existing process was touched") from None


def launch_server(values, path):
    values = validate_values("local", values)
    binary = server_binary(); free_port(); initialize_runtime(path)
    env = {k: v for k, v in os.environ.items() if k not in FIELDS and k != "META_SERVER_E2E_SIGNALS"}
    env.update(values)
    env["RUST_LOG"] = "meta_server=info,meta_server_node::server::server_app=info"
    log = open(no_symlinks(path / "server.log"), "ab", buffering=0)
    os.chmod(path / "server.log", 0o600)
    try:
        child = subprocess.Popen([str(binary)], cwd=path, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log, umask=0o077)
    finally:
        log.close()
    return child


def stop_owned(child, deadline=None):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=min(5, max(0, deadline - time.monotonic() - 0.2)) if deadline else 5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=max(0.05, min(0.2, deadline - time.monotonic())) if deadline else 1)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def local_http(path, data=None, deadline=None):
    timeout = min(2, deadline - time.monotonic()) if deadline else 2
    if timeout <= 0:
        raise ConfigError("Local smoke aggregate deadline exceeded")
    request = urllib.request.Request("http://127.0.0.1:3000" + path, data=json.dumps(data).encode() if data is not None else None,
                                     headers={"Content-Type": "application/json", "Origin": "http://localhost:5174"})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(request, timeout=timeout) as response:
            body = response.read(16385)
            if len(body) > 16384:
                raise ConfigError("Oversized local HTTP response")
            return response.status, body
    except urllib.error.HTTPError as error:
        return error.code, b""


def wait_ready(child, path, offset, deadline):
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise ConfigError("Owned server exited before readiness")
        with (path / "server.log").open("rb") as log:
            log.seek(offset)
            text = log.read(65536).decode(errors="replace")
        if "panicked at" in text:
            raise ConfigError("Owned server background initialization failed")
        if "Server initialized with device:" in text and database_count(path / "meta-secret.db") > 0:
            try:
                if local_http("/hi", deadline=deadline)[0] == 200 and child.poll() is None:
                    return
            except (OSError, urllib.error.URLError):
                pass
        time.sleep(0.025)  # retry an actual readiness condition, not a settle delay
    raise ConfigError("Owned background/SQLite/HTTP readiness deadline exceeded")


def local_smoke():
    server_binary(); free_port(); safe_dir(RUNTIME / "smoke")
    path = Path(tempfile.mkdtemp(prefix="owned-", dir=RUNTIME / "smoke"))
    values = {FIELDS[0]: "org.example.infra-smoke", FIELDS[1]: "https://apple-dev.meta-secret.org/apple/callback", FIELDS[2]: ORIGINS["local"]}
    deadline = time.monotonic() + 30
    prior = None
    for cycle in range(2):
        if time.monotonic() >= deadline - 2:
            raise ConfigError("Local smoke aggregate deadline exceeded before launch")
        offset = (path / "server.log").stat().st_size if (path / "server.log").exists() else 0
        child = launch_server(values, path)
        try:
            wait_ready(child, path, offset, min(deadline - 2, time.monotonic() + 15))
            verifier = base64.urlsafe_b64encode(bytes(range(32))).decode().rstrip("=")
            challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
            status, body = local_http("/apple/start", {"platform": "web", "challenge": challenge}, deadline - 2)
            result = json.loads(body)
            if status != 200 or not result["authorizationUrl"].startswith("https://appleid.apple.com/auth/authorize?"):
                raise ConfigError("Owned broker startup failed")
            proof = {"requestId": result["requestId"], "verifier": verifier}
            if local_http("/apple/cancel", proof, deadline - 2)[0] != 200 or json.loads(local_http("/apple/take", proof, deadline - 2)[1]) != {"status": "cancelled"} or local_http("/apple/take", proof, deadline - 2)[0] != 410:
                raise ConfigError("Owned broker cancellation/replay check failed")
            if child.poll() is not None:
                raise ConfigError("Owned server exited during smoke")
        finally:
            stop_owned(child, deadline)
        if time.monotonic() >= deadline:
            raise ConfigError("Local smoke aggregate deadline exceeded")
        key = (path / "master_key.json").stat()
        snapshot = (database_count(path / "meta-secret.db"), key.st_ino, key.st_size, key.st_mtime_ns)
        if prior is not None and prior != snapshot:
            raise ConfigError("Repeat startup changed the owned DB count or key-file identity")
        prior = snapshot
        print(json.dumps({"cycle": cycle + 1, "background": "fresh marker", "broker": "start/cancel/take/replay passed", "ownedChild": "stopped", "rows": snapshot[0]}), flush=True)
    if time.monotonic() >= deadline:
        raise ConfigError("Local smoke aggregate deadline exceeded after cleanup")
    print(json.dumps({"status": "PASS", "starts": 2, "background": "fresh per-child marker", "sqlite": "migration-backed; repeat preserved", "broker": "start/cancel/take/replay", "network": "127.0.0.1 only; no Apple/tunnel", "process": "owned child stopped", "elapsedSeconds": round(30 - (deadline - time.monotonic()), 3)}))


def verify_tools():
    cloudflared = os.environ.get("CLOUDFLARED_BIN", "cloudflared")
    kcl = os.environ.get("KCL_BIN", "kcl")
    environment = tool_environment()
    for binary in [cloudflared, kcl]:
        result = subprocess.run([binary, "--version"], env=environment, capture_output=True, text=True, timeout=15)
        print((result.stdout + result.stderr).strip())
        if result.returncode:
            raise ConfigError("Offline tool version failed; diagnostic above")
    safe_dir(RUNTIME / "checks")
    config = RUNTIME / "checks/cloudflared.yml"
    atomic_write(config, json.dumps(tunnel_config("12345678-1234-4234-8234-123456789abc")))
    command = [cloudflared, "--no-autoupdate", "tunnel", "--config", str(config), "ingress"]
    subprocess.run(command + ["validate"], capture_output=True, text=True, check=True, timeout=15)
    cases = [(HOSTS["local"], path, i) for i, path in enumerate(PATHS)]
    cases += [(HOSTS["local"], path, 4) for path in ["/", "/hi", "/meta_request", "/state-events", "/join-events", "/apple/start/", "/apple/start-extra", "/Apple/start", "/apple/start/%2e%2e/meta_request"]]
    cases += [("evil.test", "/apple/start", 4)]
    for host, path, expected in cases:
        result = subprocess.run(command + ["rule", "https://" + host + path], capture_output=True, text=True, check=True, timeout=15)
        match = re.search(r"Matched rule #(\d+)", result.stdout + result.stderr)
        if not match or int(match[1]) != expected:
            raise ConfigError("Actual Cloudflared routing did not match the fixed allowlist")
    print(f"PASS: real Cloudflared validate and {len(cases)} ingress matches")
    for values in [{}, {FIELDS[0]: "org.example.infra", FIELDS[1]: "https://api.meta-secret.org/apple/callback", FIELDS[2]: ORIGINS["production"]}]:
        rendered = json.loads(render_manifest(values, module=True))["cfg"]
        container = rendered["spec"]["template"]["spec"]["containers"][0]
        if rendered["spec"]["replicas"] != 1 or container.get("env", []) != [{"name": k, "value": values[k]} for k in FIELDS if k in values]:
            raise ConfigError("Actual KCL env/replica render mismatch")
        output = render_manifest(values)
        atomic_write(RUNTIME / "checks" / ("production.yaml" if values else "disabled.yaml"), output)
    partial = kcl_command({})
    index = next(i for i, value in enumerate(partial) if value.startswith("appleEmailServiceId="))
    partial[index] = 'appleEmailServiceId="partial"'
    result = subprocess.run(partial, env=environment, capture_output=True, text=True, timeout=60)
    if result.returncode == 0:
        raise ConfigError("Actual KCL accepted partial Apple options")
    print("PASS: real KCL full YAML + StatefulSet JSON disabled/configured; partial rejected")


def main(arguments=None):
    args = list(sys.argv[1:] if arguments is None else arguments)
    if not args:
        raise ConfigError("Select a documented infrastructure command")
    command = args.pop(0)
    if command == "deploy":
        deploy(args); return
    if args:
        raise ConfigError("Unexpected arguments; task inputs use documented environment pointers")
    if command == "install-production":
        install_production(); return
    if command == "verify":
        verify_tools(); return
    if command == "local-smoke":
        local_smoke(); return
    if command == "render-process":
        values = validate_values("production", {k: os.environ[k] for k in FIELDS if k in os.environ})
        atomic_write(INFRA / "k8s/meta-secret-server/build/meta-server.yaml", render_manifest(values)); return
    name, values = selected_profile()
    if command == "check":
        print(json.dumps({"profile": name, "apple": "configured" if values else "disabled", "coreSelection": "unchanged; independent from Apple profile"}))
    elif command == "prepare":
        prepare(name, values, os.environ.get("APPLE_TUNNEL_ID"))
        print("Prepared ignored public exports; no client/Core selection changed or tunnel opened")
    elif command == "render":
        if values and name != "production":
            raise ConfigError("Kubernetes render requires production or disabled profile")
        atomic_write(INFRA / "k8s/meta-secret-server/build/meta-server.yaml", render_manifest(values))
    elif command == "local-run":
        if name != "local":
            raise ConfigError("Local server cannot use production profile")
        path = RUNTIME / "local/server"
        offset = (path / "server.log").stat().st_size if (path / "server.log").exists() else 0
        child = launch_server(values, path)
        try:
            wait_ready(child, path, offset, time.monotonic() + 15)
            print("Local SQLite/background/HTTP ready; Core/client selectors unchanged", flush=True)
            child.wait()
        finally:
            stop_owned(child)
    else:
        raise ConfigError("Unknown infrastructure command")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
    except (ConfigError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(str(error) if isinstance(error, ConfigError) else "Local infrastructure operation failed; no payload logged", file=sys.stderr)
        sys.exit(1)
