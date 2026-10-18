"""Behavior tests; real compiler/ingress/runtime checks have separate Task receipts."""
import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

CORE = Path(__file__).resolve().parents[2]
WRAPPER = CORE.parent / "scripts/deploy-server.sh"
FIELDS = ("APPLE_EMAIL_SERVICE_ID", "APPLE_EMAIL_CALLBACK_URL", "APPLE_EMAIL_WEB_ORIGINS")


def load():
    spec = importlib.util.spec_from_file_location("apple_infra_test_subject", Path(__file__).with_name("apple_infra.py"))
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def profile(name="local"):
    host = "apple-dev.meta-secret.org" if name == "local" else "api.meta-secret.org"
    origins = "http://localhost:5173,http://localhost:5174" if name == "local" else "https://meta-secret.github.io"
    return f"{FIELDS[0]}=org.example.infra\n{FIELDS[1]}=https://{host}/apple/callback\n{FIELDS[2]}={origins}\n"


class DeploymentBoundary(unittest.TestCase):
    def test_root_dry_run_rejects_branch_injection(self):
        a = load()
        with patch.object(a.subprocess, "run") as run:
            with self.assertRaises(a.ConfigError): a.deploy(["--dry-run", "codex/example;printf INJECTED"])
            run.assert_not_called()

    def test_actual_dry_run_never_executes_remote_or_push_commands(self):
        a = load(); calls = []
        def boundary(command, **kwargs):
            calls.append(command)
            if command[:2] == ["git", "check-ref-format"]:
                return subprocess.CompletedProcess(command, 0)
            raise AssertionError("Intercepted external command (including absolute SSH)")
        output = io.StringIO()
        with patch.object(a.subprocess, "run", side_effect=boundary), contextlib.redirect_stdout(output):
            a.deploy(["--dry-run", "codex/orbit-registration"])
        data = json.loads(output.getvalue())
        self.assertEqual(data["apple"], "disabled")
        self.assertLess(data["steps"].index("task taskomatic"), data["steps"].index("taskomatic k8s:deploy_meta_server"))
        self.assertEqual(len(calls), 1)

    def test_thin_wrapper_preserves_arguments_without_executing_helper(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp).resolve()
            calls = folder / "calls"; python = folder / "python3"
            python.write_text('#!/bin/bash\nprintf "%s\\0" "$@" > "$BOUNDARY_CALLS"\n')
            python.chmod(0o700)
            self.assertNotRegex(WRAPPER.read_text(), r'\b(ssh|docker|kubectl|curl|wget)\b')
            env = {**os.environ, "PATH": str(folder) + os.pathsep + os.environ["PATH"], "BOUNDARY_CALLS": str(calls)}
            args = ["--dry-run", "codex/argument;literal"]
            result = subprocess.run(["bash", str(WRAPPER), *args], env=env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(calls.read_bytes().decode().split('\0')[:-1], [str(CORE / "infra/apple/apple_infra.py"), "deploy", *args])

    def test_executable_remote_freshness_guards_fail_dirty_or_wrong_head_before_build(self):
        a = load(); prefix = a.REMOTE_PROGRAM.split("printf '%s'")[0]
        self.assertNotRegex(prefix, r'\b(ssh|docker|kubectl|task|k3d)\b')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve(); (root / "meta/meta-secret-core").mkdir(parents=True)
            binary = root / "git"
            binary.write_text('''#!/bin/bash
case "$1" in
 status) [ "$FRESHNESS_CASE" != dirty ] || printf ' M tracked-file\n' ;;
 rev-parse) if [ "$2" = HEAD ] && [ "$FRESHNESS_CASE" = mismatch ]; then echo different; else echo selected; fi ;;
 *) exit 0 ;;
esac
exit 0
''')
            binary.chmod(0o700)
            for case in ["dirty", "mismatch", "clean"]:
                env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"], "FRESHNESS_CASE": case}
                result = subprocess.run(["bash", "-s", "--", "codex/orbit-registration", "public-payload"], input=prefix, text=True, cwd=root, env=env, capture_output=True, timeout=5)
                self.assertEqual(result.returncode == 0, case == "clean", (case, result.stderr))


class Profiles(unittest.TestCase):
    def setUp(self):
        self.a = load()

    def test_disabled_omits_all_three_names(self):
        for text in ["", "\n# disabled\n", "\n".join(k + "=" for k in FIELDS)]:
            self.assertEqual(self.a.parse_profile("local", text), {})
        self.assertEqual(self.a.exports("local", {})["server.env"], "")
        self.assertEqual(self.a.exports("local", {})["web.env"], "VITE_APPLE_EMAIL_BROKER_URL=\n")

    def test_valid_profiles_fix_callback_and_origins(self):
        for name in ["local", "production"]:
            values = self.a.parse_profile(name, profile(name))
            self.assertEqual(set(values), set(FIELDS))
            self.assertEqual(len(self.a.exports(name, values)), 3)
        self.assertNotIn("meta-secret.github.io", self.a.parse_profile("local", profile())[FIELDS[2]])

    def test_partial_duplicate_unknown_oversize_and_injected_profiles_fail(self):
        bad = ["APPLE_EMAIL_SERVICE_ID=x", profile() + "APPLE_EMAIL_SERVICE_ID=again\n", profile() + "PASSWORD=secret\n", "x" * 16385,
               profile().replace("org.example.infra", "$(touch /tmp/no)"), profile().replace("org.example.infra", "REPLACE_ME"),
               profile().replace("apple-dev.meta-secret.org", "evil.test"), profile().replace("5174", "5175"), profile("production"),
               profile().replace("org.example.infra", '"org.example.infra"'), profile().replace("org.example.infra", "x\x00y")]
        for text in bad:
            with self.subTest(textClass=len(text)), self.assertRaises(self.a.ConfigError):
                self.a.parse_profile("local", text)
        with self.assertRaises(self.a.ConfigError):
            self.a.parse_profile("local;echo", profile())

    def test_branch_validation_precedes_commands_for_bad_inputs(self):
        with patch.object(self.a.subprocess, "run") as run:
            for branch in ["-bad", "HEAD", "x;echo", "x$(id)", "x\ny", "x'bad", "a" * 256]:
                with self.assertRaises(self.a.ConfigError):
                    self.a.validate_branch(branch)
            run.assert_not_called()
        self.assertEqual(self.a.validate_branch("codex/orbit-registration"), "codex/orbit-registration")

    def test_process_values_cannot_hide_newlines_as_comments(self):
        values = self.a.parse_profile("production", profile("production"))
        values[FIELDS[0]] += "\n# comment"
        with self.assertRaises(self.a.ConfigError): self.a.validate_values("production", values)

    def test_partial_process_env_never_reaches_compiler_or_output(self):
        with patch.dict(os.environ, {FIELDS[0]: "partial"}, clear=True), patch.object(self.a.subprocess, "run") as run, patch.object(self.a, "atomic_write") as write:
            with self.assertRaises(self.a.ConfigError): self.a.main(["render-process"])
            run.assert_not_called(); write.assert_not_called()

    def test_local_launch_clears_ambient_apple_and_uses_owned_cwd_without_selector_writes(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp).resolve()
            with patch.object(self.a, "server_binary", return_value=Path("/synthetic/server")), patch.object(self.a, "free_port"), patch.object(self.a, "initialize_runtime"), patch.object(self.a.subprocess, "Popen") as popen, patch.dict(os.environ, {k: "ambient-invalid" for k in FIELDS}):
                self.a.launch_server({}, path)
                args, kwargs = popen.call_args
                self.assertEqual(args[0], ["/synthetic/server"])
                self.assertEqual(kwargs["cwd"], path)
                self.assertTrue(all(k not in kwargs["env"] for k in FIELDS))
                self.assertEqual(kwargs["umask"], 0o077)
                self.assertEqual({p.name for p in path.iterdir()}, {"server.log"})

    def test_install_payload_is_bounded_unique_public_data_and_disabled_omits_vars(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(self.a, "RUNTIME", Path(tmp).resolve() / "runtime"):
            for data in [self.a.parse_profile("production", profile("production")), {}]:
                with patch.object(sys, "stdin", io.TextIOWrapper(io.BytesIO(base64.urlsafe_b64encode(json.dumps(data).encode())))):
                    self.a.install_production()
                text = (self.a.RUNTIME / "production/server.env").read_text()
                self.assertEqual(bool(text), bool(data))
            for raw in [b'{"APPLE_EMAIL_SERVICE_ID":"a","APPLE_EMAIL_SERVICE_ID":"b"}', b'{"PRIVATE_KEY":"not-allowed"}', b'[]']:
                with patch.object(sys, "stdin", io.TextIOWrapper(io.BytesIO(base64.urlsafe_b64encode(raw)))):
                    with self.assertRaises(self.a.ConfigError): self.a.install_production()

    def test_private_runtime_is_gitignored_and_docker_context_excluded(self):
        result = subprocess.run(["git", "check-ignore", "infra/.apple/credentials/synthetic.json"], cwd=CORE, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertIn(".apple/", (CORE / "infra/.dockerignore").read_text().splitlines())

    def test_prepare_writes_only_private_owned_exports_and_never_reads_credentials(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(self.a, "RUNTIME", Path(tmp).resolve() / "runtime"):
            self.a.prepare("local", self.a.parse_profile("local", profile()), "12345678-1234-4234-8234-123456789abc")
            path = self.a.RUNTIME / "local"
            self.assertEqual(set(p.name for p in path.iterdir()), {"server.env", "web.env", "android.properties", "cloudflared.yml"})
            self.assertEqual(path.stat().st_mode & 0o777, 0o700)
            self.assertEqual((path / "server.env").stat().st_mode & 0o777, 0o600)
            self.assertNotIn("META_SECRET", (path / "web.env").read_text())
            self.assertNotIn("TOKEN", (path / "cloudflared.yml").read_text())
            self.assertFalse(any((self.a.RUNTIME / "credentials").glob("*.json")))

    def test_symlink_parent_and_leaf_are_rejected_without_touching_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve(); target = root / "target"; target.mkdir()
            link = root / "link"; link.symlink_to(target, target_is_directory=True)
            for path in [link / "file", link]:
                with self.assertRaises(self.a.ConfigError): self.a.atomic_write(path, "bad")
            leaf = root / "leaf"; leaf.symlink_to(target / "secret")
            with self.assertRaises(self.a.ConfigError): self.a.atomic_write(leaf, "bad")
            self.assertEqual(list(target.iterdir()), [])

    def test_tunnel_has_four_exact_paths_then_deny_all_and_no_secret(self):
        config = self.a.tunnel_config("12345678-1234-4234-8234-123456789abc")
        self.assertEqual([r["path"] for r in config["ingress"][:-1]], ["^/apple/start$", "^/apple/callback$", "^/apple/take$", "^/apple/cancel$"])
        self.assertTrue(all(r["hostname"] == "apple-dev.meta-secret.org" and r["service"] == "http://127.0.0.1:3000" for r in config["ingress"][:-1]))
        self.assertEqual(config["ingress"][-1], {"service": "http_status:404"})
        with self.assertRaises(self.a.ConfigError): self.a.tunnel_config("../../secret")

    def test_invalid_config_does_not_create_output_or_invoke_a_tool(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(self.a, "RUNTIME", Path(tmp).resolve() / "runtime"), patch.object(self.a.subprocess, "run") as run:
            with self.assertRaises(self.a.ConfigError): self.a.prepare("local", {FIELDS[0]: "partial"})
            self.assertFalse(self.a.RUNTIME.exists()); run.assert_not_called()

    def test_deploy_plan_is_static_and_payload_has_only_public_validated_names(self):
        values = self.a.parse_profile("production", profile("production"))
        command, program = self.a.remote_deploy("codex/orbit-registration", values)
        self.assertEqual(command[:2], ["/usr/bin/ssh", "debian@188.165.236.156"])
        self.assertIn('git check-ref-format --branch "$1"', program)
        self.assertLess(program.index("task taskomatic"), program.index("k8s:deploy_meta_server"))
        self.assertNotIn("docker buildx", program)
        self.assertNotIn("org.example.infra", program)
        payload = command[-1].split()[-1]
        self.assertEqual(json.loads(base64.urlsafe_b64decode(payload)), values)


class OwnedSQLite(unittest.TestCase):
    def setUp(self): self.a = load()

    def test_actual_migration_initializes_fresh_schema_and_valid_repeat_preserves_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            runtime = Path(tmp).resolve() / "server"
            self.a.initialize_runtime(runtime)
            database = runtime / "meta-secret.db"; receipt = runtime / "owner.json"
            with contextlib.closing(sqlite3.connect(database)) as conn, conn:
                self.assertEqual([r[1] for r in conn.execute('PRAGMA table_info(db_commit_log)')], ["id", "key_id", "event"])
                conn.execute('INSERT INTO db_commit_log VALUES(1,?,?)', ("synthetic", "synthetic"))
            key = runtime / "master_key.json"; key.write_text("synthetic owned key stub")
            old = (database.read_bytes(), receipt.read_bytes(), key.stat().st_mtime_ns)
            self.a.initialize_runtime(runtime)
            self.assertEqual(old, (database.read_bytes(), receipt.read_bytes(), key.stat().st_mtime_ns))
            migration = CORE / "meta-secret/db/sqlite/migrations/2023-04-22-065820_create_commit_log/up.sql"
            self.assertIn(hashlib.sha256(migration.read_bytes()).hexdigest(), receipt.read_text())

    def test_unowned_missing_key_bad_schema_changed_provenance_and_symlinks_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            for case in ["unowned", "key_missing", "schema", "receipt", "symlink"]:
                runtime = root / case
                if case == "unowned":
                    runtime.mkdir(); (runtime / "meta-secret.db").write_text("not ours")
                else:
                    self.a.initialize_runtime(runtime)
                    if case == "key_missing":
                        with contextlib.closing(sqlite3.connect(runtime / "meta-secret.db")) as conn, conn: conn.execute('INSERT INTO db_commit_log VALUES(1,"synthetic","synthetic")')
                    elif case == "schema":
                        with contextlib.closing(sqlite3.connect(runtime / "meta-secret.db")) as conn, conn: conn.execute('DROP TABLE db_commit_log')
                    elif case == "receipt": (runtime / "owner.json").write_text('{}')
                    else:
                        (runtime / "meta-secret.db").unlink(); (runtime / "meta-secret.db").symlink_to(root / "foreign.db")
                with self.subTest(case=case), self.assertRaises(self.a.ConfigError): self.a.initialize_runtime(runtime)

    def test_old_startup_marker_and_http200_cannot_make_new_child_ready(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp).resolve(); old = b'Server initialized with device: prior\n'
            (path / "server.log").write_bytes(old)
            child = Mock(); child.poll.return_value = None
            with patch.object(self.a, "database_count", return_value=1), patch.object(self.a, "local_http", return_value=(200, b"ok")) as http, patch.object(self.a.time, "monotonic", side_effect=[0, 2]), patch.object(self.a.time, "sleep"):
                with self.assertRaises(self.a.ConfigError): self.a.wait_ready(child, path, len(old), 1)
                http.assert_not_called()
            (path / "server.log").write_bytes(old + b'background panicked at fixture\n')
            with patch.object(self.a.time, "monotonic", return_value=0):
                with self.assertRaises(self.a.ConfigError): self.a.wait_ready(child, path, len(old), 1)

    def test_expired_http_budget_opens_no_connection_and_shutdown_targets_only_child(self):
        with patch.object(self.a.time, "monotonic", return_value=10), patch.object(self.a.urllib.request, "build_opener") as opener:
            with self.assertRaises(self.a.ConfigError): self.a.local_http("/hi", deadline=9)
            opener.assert_not_called()
        child = Mock(); child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("owned", 0), 0]
        with patch.object(self.a.time, "monotonic", return_value=10): self.a.stop_owned(child, deadline=11)
        child.terminate.assert_called_once(); child.kill.assert_called_once()
        self.assertLessEqual(child.wait.call_args_list[0].kwargs['timeout'], 1)

    def test_preflight_reuses_time_wait_but_rejects_real_bind_conflict(self):
        with patch.object(self.a.socket, "socket") as factory:
            listener = factory.return_value.__enter__.return_value
            self.a.free_port()
            listener.setsockopt.assert_called_once_with(self.a.socket.SOL_SOCKET, self.a.socket.SO_REUSEADDR, 1)
            listener.listen.assert_called_once_with(1)
            listener.bind.side_effect = OSError(self.a.errno.EADDRINUSE, "fixture")
            with self.assertRaises(self.a.ConfigError): self.a.free_port()


if __name__ == "__main__": unittest.main()
