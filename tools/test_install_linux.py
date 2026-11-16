"""Installer contracts run under Bash with mocked native tools, even on Windows."""
import hashlib
import os
from pathlib import Path
import tarfile
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "dist" / "install-linux.sh"
BASH = shutil.which("bash")


def shell_path(path):
    value = str(path).replace("\\", "/")
    if len(value) > 1 and value[1] == ":":
        return "/" + value[0].lower() + value[2:]
    return value


@unittest.skipUnless(BASH, "Bash is required")
class LinuxInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="aoxn-linux-install-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "source"
        self.home = self.base / "home"
        self.tools = self.base / "tools"
        self.prefix = self.base / "installed"
        for directory in (self.root / "dist", self.home, self.tools,
                          self.root / "target/release", self.root / "stdlib/net",
                          self.root / "examples/nested", self.root / "docs"):
            directory.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SCRIPT, self.root / "dist/install-linux.sh")
        for name, content in {
            "stdlib/core.ax": "core\n", "stdlib/net/native.rs": "bridge\n",
            "examples/hello.ax": "hello\n", "examples/nested/native.rs": "example bridge\n",
            "docs/install.md": "install\n", "LICENSE": "license\n",
            "README.md": "readme\n", "CHANGELOG.md": "changes\n",
        }.items():
            (self.root / name).write_text(content, encoding="utf-8")
        self.executable(self.tools / "uname", 'printf "%s\\n" "${MOCK_OS:-Linux}"\n')
        self.executable(self.tools / "clang", 'exit 0\n')
        self.executable(self.tools / "cargo", 'printf "%s\\n" "$*" >> "$CARGO_LOG"\nexit "${CARGO_STATUS:-0}"\n')
        self.executable(self.root / "target/release/aoxn",
                        '[ "$1" = doctor ] || exit 91\n'
                        'printf "%s\\n" "$AOXN_HOME" > "$DOCTOR_LOG"\n'
                        'exit "${DOCTOR_STATUS:-0}"\n')
        self.env = os.environ.copy()
        # Bash's inherited system PATH supplies cp/install/realpath, not toolchains.
        native_path = subprocess.run([BASH, "-c", 'printf "%s" "$PATH"'],
                                     capture_output=True, text=True, check=True).stdout
        self.env.update(HOME=shell_path(self.home),
                        PATH=shell_path(self.tools) + ":" + native_path,
                        CARGO_LOG=shell_path(self.base / "cargo.log"),
                        DOCTOR_LOG=shell_path(self.base / "doctor.log"),
                        MOCK_OS="Linux", DOCTOR_STATUS="0", CARGO_STATUS="0")
        for name in (".profile", ".bashrc", ".bash_profile", ".zshrc"):
            (self.home / name).write_text("# original\n", encoding="utf-8")

    def executable(self, path, body):
        path.write_text("#!/usr/bin/env bash\n" + body, encoding="utf-8")
        path.chmod(0o755)

    def run_installer(self, *args, prefix=True):
        command = [BASH, shell_path(self.root / "dist/install-linux.sh")]
        if prefix:
            command += ["--prefix", shell_path(self.prefix)]
        return subprocess.run(command + list(args), env=self.env,
                              capture_output=True, text=True, timeout=30)

    def assert_failed_without_writes(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.prefix.exists())
        self.assertFalse((self.base / "cargo.log").exists())
        self.assertFalse((self.base / "doctor.log").exists())

    def test_missing_and_unknown_arguments(self):
        for args in (("--prefix",), ("--prefix", "--skip-build"), ("--bogus",)):
            with self.subTest(args=args):
                self.assert_failed_without_writes(self.run_installer(*args, prefix=False))

    def test_unsafe_prefixes_rejected_before_writes(self):
        for value in ("", "/", "///", "/tmp/..", "/usr", "/usr/local",
                      "relative", shell_path(self.home), shell_path(self.root),
                      shell_path(self.root / "stdlib/nested-install"),
                      shell_path(self.root / "examples/nested-install"), shell_path(self.base),
                      "/tmp/bad\npath", "/tmp/bad:path"):
            with self.subTest(prefix=value):
                self.assert_failed_without_writes(
                    self.run_installer("--prefix=" + value, prefix=False))

    def test_non_linux_rejected_but_help_allowed(self):
        self.env["MOCK_OS"] = "Darwin"
        self.assert_failed_without_writes(self.run_installer())
        result = self.run_installer("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Usage:", result.stdout)
        self.assertFalse(self.prefix.exists())

    def test_build_full_copy_and_profiles_unchanged_by_default(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.base / "cargo.log").read_text().strip(),
                         "build --release --locked --bin aoxn")
        for source_dir, destination in (("stdlib", "lib/stdlib"), ("examples", "examples")):
            for source in (self.root / source_dir).rglob("*"):
                if source.is_file():
                    target = self.prefix / destination / source.relative_to(self.root / source_dir)
                    self.assertEqual(target.read_bytes(), source.read_bytes())
        self.assertTrue((self.prefix / "bin/aoxn").is_file())
        for profile in self.home.iterdir():
            self.assertEqual(profile.read_text(), "# original\n")
        self.assertEqual((self.base / "doctor.log").read_text().strip(), shell_path(self.prefix))

    def test_skip_build_and_reinstall_preserves_unrelated_files(self):
        (self.prefix / "lib/stdlib").mkdir(parents=True)
        sentinel = self.prefix / "lib/stdlib/unrelated.txt"
        sentinel.write_text("keep me")
        result = self.run_installer("--skip-build", "--no-modify-path")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.base / "cargo.log").exists())
        self.assertEqual(sentinel.read_text(), "keep me")

    def test_doctor_failure_is_failure_and_does_not_edit_profile(self):
        self.env["DOCTOR_STATUS"] = "17"
        result = self.run_installer("--modify-path")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("doctor failed", result.stderr)
        self.assertNotIn("==> installed Aoxn", result.stdout)
        self.assertEqual((self.home / ".profile").read_text(), "# original\n")

    def test_profile_shell_quoting_and_idempotence(self):
        # Executing the profile must preserve literal shell metacharacters.
        self.prefix = self.base / "space ' quote $HOME `uname` $(uname)"
        for _ in range(2):
            result = self.run_installer("--skip-build", "--modify-path")
            self.assertEqual(result.returncode, 0, result.stderr)
        profile = self.home / ".profile"
        self.assertEqual(profile.read_text().count("# added by Aoxn"), 1)
        result = subprocess.run(
            [BASH, "-c", '. "$HOME/.profile"; printf "%s" "$PATH"'],
            env=self.env, capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout.split(":")[0], shell_path(self.prefix / "bin"))
        self.assertEqual((self.base / "doctor.log").read_text().strip(), shell_path(self.prefix))

    def test_explicit_no_modify_path_overrides_opt_in(self):
        result = self.run_installer("--modify-path", "--no-modify-path", "--skip-build")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.home / ".profile").read_text(), "# original\n")

    def test_build_failure_does_not_install(self):
        self.env["CARGO_STATUS"] = "23"
        result = self.run_installer()
        self.assertEqual(result.returncode, 23)
        self.assertFalse(self.prefix.exists())

    def test_binary_package_installs_without_cargo(self):
        (self.root / "bin").mkdir()
        (self.root / "lib").mkdir()
        shutil.move(self.root / "target/release/aoxn", self.root / "bin/aoxn")
        shutil.move(self.root / "stdlib", self.root / "lib/stdlib")
        self.env["CARGO_STATUS"] = "23"
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.base / "cargo.log").exists())
        self.assertEqual((self.prefix / "lib/stdlib/net/native.rs").read_text(), "bridge\n")

    def test_packaging_contains_whole_tree_and_matching_digest(self):
        shutil.copyfile(SCRIPT.with_name("package-linux.sh"), self.root / "dist/package-linux.sh")
        (self.root / "SECURITY.md").write_text("security\n", encoding="utf-8")
        self.executable(self.tools / "uname",
                        'if [ "$1" = -m ]; then printf "x86_64\\n"; else printf "Linux\\n"; fi\n')
        self.executable(self.root / "target/release/aoxn",
                        'if [ "$1" = version ]; then printf "aoxn 0.55.0\\n"; exit 0; fi\nexit 91\n')
        output = self.base / "packages with spaces"
        command = [BASH, shell_path(self.root / "dist/package-linux.sh"),
                   "--skip-build", "--output", shell_path(output)]
        result = subprocess.run(command, env=self.env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = output / "Aoxn-0.55.0-linux-x86_64.tar.gz"
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assertEqual(Path(str(archive) + ".sha256").read_text().split()[0], digest)
        with tarfile.open(archive) as package:
            names = set(package.getnames())
            for name in ("bin/aoxn", "lib/stdlib/net/native.rs", "examples/nested/native.rs",
                         "dist/install-linux.sh", "SECURITY.md"):
                self.assertIn("Aoxn-0.55.0-linux-x86_64/" + name, names)
        before = archive.read_bytes()
        repeated = subprocess.run(command, env=self.env, capture_output=True, text=True, timeout=30)
        self.assertNotEqual(repeated.returncode, 0)
        self.assertEqual(archive.read_bytes(), before)
        self.assertFalse((self.base / "cargo.log").exists())

    def test_skip_build_requires_existing_compiler(self):
        (self.root / "target/release/aoxn").unlink()
        self.assert_failed_without_writes(self.run_installer("--skip-build"))


if __name__ == "__main__":
    unittest.main()
