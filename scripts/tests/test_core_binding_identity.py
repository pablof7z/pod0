"""The Apple core freshness gate must compare against the built library.

Issue #188: scripts/build_pod0_core_apple.sh stamps the xcframework by copying
Generated/Pod0Core/bindings.fingerprint, so check_core_binding_freshness.sh
compares the committed fingerprint to a copy of itself and can never fail.
The stamp must instead be derived from the library that was just built.

These tests run the real scripts inside a temporary replica repository.
The cargo toolchain is replaced by PATH shims so that the "built library"
is a deterministic artifact encoding a fixture record layout, and binding
generation derives Swift text from that artifact. This mirrors the uniffi
library-mode dataflow, which the issue investigation verified to be
byte-deterministic across the host dylib and both iOS static library slices,
while keeping the whole pipeline to subprocess overhead instead of three
iOS target builds. Normalization, fingerprinting, stamping, and the gate
itself all run through the production scripts unmodified.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_MAGIC = b"pod0-fixture-archive-v1\n"
SWIFT_TYPES = {"String": "String", "i64": "Int64", "u32": "UInt32"}
HEADER_TEXT = "// pod0 fixture FFI header\n"

FIXTURE_SOURCE = """\
// Fixture facade: each record's field order is its FFI wire layout.
pub struct HostRequestEnvelope {
    pub request_id: String,
    pub deadline_at: i64,
    pub issued_revision: u32,
}

pub struct AgentApprovalObserved {
    pub approved: u32,
    pub occurred_at: i64,
}
"""

SWAPPED_FIELDS = (
    "    pub deadline_at: i64,\n    pub issued_revision: u32,\n",
    "    pub issued_revision: u32,\n    pub deadline_at: i64,\n",
)


def _argument_value(args: list[str], flag: str) -> str | None:
    for index, arg in enumerate(args[:-1]):
        if arg == flag:
            return args[index + 1]
    return None


def _derive_bindings(library: Path) -> str:
    """Regenerate binding text from the layout compiled into the artifact."""
    source = library.read_bytes().removeprefix(ARCHIVE_MAGIC).decode()
    lines = ["// Derived from the compiled library; field order is layout."]
    for raw in source.splitlines():
        line = raw.strip()
        record = re.fullmatch(r"pub struct (\w+) \{", line)
        field = re.fullmatch(r"pub (\w+): (\w+),", line)
        if record:
            lines.append(f"public struct {record.group(1)} {{")
        elif field:
            swift_type = SWIFT_TYPES[field.group(2)]
            lines.append(f"    public let {field.group(1)}: {swift_type}")
        elif line == "}":
            lines.append("}")
    return "\n".join(lines) + "\n"


def _shim_cargo(args: list[str]) -> int:
    workspace = Path.cwd()
    command = args[0] if args else ""
    if command == "rustc":
        target = _argument_value(args, "--target")
        release = workspace / "target"
        release = release / target / "release" if target else release / "release"
        release.mkdir(parents=True, exist_ok=True)
        name = "libpod0_facade.a" if target else "libpod0_facade.dylib"
        source = (workspace / "crates/pod0-facade/src/lib.rs").read_bytes()
        (release / name).write_bytes(ARCHIVE_MAGIC + source)
        return 0
    if command == "metadata":
        print(json.dumps({"target_directory": str(workspace / "target")}))
        return 0
    if command == "run":
        library = Path(_argument_value(args, "--library"))
        out_dir = Path(_argument_value(args, "--out-dir"))
        language = _argument_value(args, "--language") or "swift"
        out_dir.mkdir(parents=True, exist_ok=True)
        if language == "swift":
            (out_dir / "pod0_fixture.swift").write_text(_derive_bindings(library))
            (out_dir / "pod0_fixtureFFI.h").write_text(HEADER_TEXT)
        else:
            (out_dir / "pod0_fixture.kt").write_text(_derive_bindings(library))
        return 0
    return 0


def _shim_lipo(args: list[str]) -> int:
    if "-create" in args:
        output = Path(_argument_value(args, "-output"))
        slices = [
            Path(arg)
            for arg in args
            if not arg.startswith("-") and Path(arg) != output and Path(arg).is_file()
        ]
        output.write_bytes(b"".join(part.read_bytes() for part in slices))
    return 0


class Replica:
    """A throwaway repository layout the production scripts run against."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.committed_fingerprint = (
            root / "Generated/Pod0Core/bindings.fingerprint"
        )
        self.stamped_fingerprint = (
            root / ".build/pod0core/Pod0CoreFFI.xcframework/bindings.fingerprint"
        )
        shims = root / "shims"
        self.env = dict(os.environ, PATH=f"{shims}:{os.environ['PATH']}")


def _write_shims(shims: Path) -> None:
    this_file = Path(__file__).resolve()
    for name, body in {
        "cargo": f'#!/bin/bash\nexec python3 "{this_file}" shim-cargo "$@"\n',
        "lipo": f'#!/bin/bash\nexec python3 "{this_file}" shim-lipo "$@"\n',
        "rustup": "#!/bin/bash\nexit 0\n",
    }.items():
        shim = shims / name
        shim.write_text(body)
        shim.chmod(0o755)


def _run(
    replica: Replica, command: list[str], cwd: Path | None = None
) -> subprocess.CompletedProcess:
    return subprocess.run(
        command,
        cwd=cwd or replica.root,
        env=replica.env,
        capture_output=True,
        text=True,
    )


def _generate_committed_bindings(replica: Replica) -> None:
    rust = replica.root / "rust"
    for command in (
        ["cargo", "rustc", "-p", "pod0-facade", "--release", "--crate-type", "cdylib"],
        [
            "cargo", "run", "-p", "pod0-uniffi-bindgen", "--", "generate",
            "--library", "target/release/libpod0_facade.dylib",
            "--language", "swift", "--no-format",
            "--out-dir", str(replica.root / "Generated/Pod0Core/Swift"),
        ],
    ):
        result = _run(replica, command, cwd=rust)
        if result.returncode != 0:
            raise RuntimeError(f"{command} failed:\n{result.stderr}")
    fingerprint = _run(
        replica,
        [
            str(replica.root / "scripts/core_bindings_fingerprint.sh"),
            str(replica.root / "Generated/Pod0Core/Swift"),
        ],
    )
    if fingerprint.returncode != 0:
        raise RuntimeError(f"fingerprinting failed:\n{fingerprint.stderr}")
    replica.committed_fingerprint.write_text(fingerprint.stdout)


def _make_replica(
    root: Path, swap_fields: bool = False, doctored: str | None = None
) -> Replica:
    replica = Replica(root)
    for sub in (
        "scripts", "shims", "Generated/Pod0Core/Swift",
        "rust/apple", "rust/crates/pod0-facade/src",
    ):
        (root / sub).mkdir(parents=True)
    for entry in sorted((REPO_ROOT / "scripts").iterdir()):
        if entry.is_file():
            shutil.copy(entry, root / "scripts" / entry.name)
    for relative in (
        "rust/uniffi.toml",
        "rust/apple/Pod0CoreFFI.modulemap",
        "rust/apple/Pod0CoreFFI.xcframework.Info.plist",
    ):
        shutil.copy(REPO_ROOT / relative, root / relative)
    lib_rs = root / "rust/crates/pod0-facade/src/lib.rs"
    lib_rs.write_text(FIXTURE_SOURCE)
    _write_shims(root / "shims")
    _generate_committed_bindings(replica)
    if swap_fields:
        lib_rs.write_text(FIXTURE_SOURCE.replace(*SWAPPED_FIELDS))
    if doctored is not None:
        replica.committed_fingerprint.write_text(doctored + "\n")
    return replica


def _build_and_check(replica: Replica) -> subprocess.CompletedProcess:
    build = _run(replica, [str(replica.root / "scripts/build_pod0_core_apple.sh")])
    if build.returncode != 0:
        raise RuntimeError(
            f"build_pod0_core_apple.sh failed:\n{build.stdout}\n{build.stderr}"
        )
    return _run(
        replica, [str(replica.root / "scripts/check_core_binding_freshness.sh")]
    )


@unittest.skipUnless(sys.platform == "darwin", "drives the Apple core build scripts")
class CoreBindingIdentityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls._temp = tempfile.TemporaryDirectory(prefix="pod0-binding-identity-")
        base = Path(cls._temp.name)
        cls.matching = _make_replica(base / "matching")
        cls.mismatched = _make_replica(base / "mismatched", swap_fields=True)
        cls.doctored = _make_replica(
            base / "doctored", swap_fields=True, doctored="0" * 64
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls._temp.cleanup()

    def test_matching_build_passes_the_freshness_gate(self) -> None:
        check = _build_and_check(self.matching)
        self.assertEqual(check.returncode, 0, msg=check.stdout + check.stderr)

    def test_layout_change_without_regeneration_is_rejected(self) -> None:
        check = _build_and_check(self.mismatched)
        self.assertNotEqual(
            check.returncode,
            0,
            msg="freshness gate passed although the built library's record "
            "layout disagrees with the committed bindings: "
            + check.stdout
            + check.stderr,
        )

    def test_stamp_is_derived_from_the_built_library_not_copied(self) -> None:
        check = _build_and_check(self.doctored)
        stamped = self.doctored.stamped_fingerprint.read_text()
        committed = self.doctored.committed_fingerprint.read_text()
        self.assertNotEqual(
            stamped,
            committed,
            msg="the xcframework stamp is a byte copy of the committed "
            "fingerprint, so a hand-edited expectation always matches itself",
        )
        self.assertNotEqual(check.returncode, 0, msg=check.stdout + check.stderr)


def _dispatch_shim(argv: list[str]) -> int:
    if argv and argv[0] == "shim-cargo":
        return _shim_cargo(argv[1:])
    if argv and argv[0] == "shim-lipo":
        return _shim_lipo(argv[1:])
    return 2


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1].startswith("shim-"):
        sys.exit(_dispatch_shim(sys.argv[1:]))
    unittest.main()
