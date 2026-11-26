import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch

import release_package as release


class ReleaseProjectionTests(unittest.TestCase):
    conversion = {
        "package": "zbridge",
        "version": "0.1.0",
        "git_sha": "d" * 40,
        "github": "https://github.com/lightjunction/zbridge",
    }

    def lock(self):
        return (
            'version = 4\n\n[[package]]\nname = "other"\nversion = "1.0.0"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
            f'checksum = "{"1" * 64}"\n\n[[package]]\nname = "zbridge"\nversion = "0.1.0"\n'
            f'source = "git+https://github.com/LIghtJUNction/zbridge.git#{"d" * 40}"\n'
            'dependencies = [ "other" ]\n'
        )

    def test_projection_replaces_only_the_locked_source(self):
        pin = "a" * 64
        first = tomllib.loads(self.lock())["package"]
        projected = tomllib.loads(
            release.projected_lock(self.lock(), [self.conversion], lambda _: pin)
        )["package"]
        self.assertEqual(first[0], projected[0])
        self.assertEqual(
            projected[1], dict(first[1], source=release.CRATES_INDEX, checksum=pin)
        )
        with self.assertRaises(ValueError):
            release.projected_lock(
                self.lock().replace("zbridge.git", "other.git"),
                [self.conversion],
                lambda _: pin,
            )

    def test_registry_checks_hash_yank_and_locked_revision(self):
        def crate(commit):
            stream = io.BytesIO()
            body = json.dumps({"git": {"sha1": commit}}).encode()
            with tarfile.open(fileobj=stream, mode="w:gz") as tar:
                member = tarfile.TarInfo("zbridge-0.1.0/.cargo_vcs_info.json")
                member.size = len(body)
                tar.addfile(member, io.BytesIO(body))
            return stream.getvalue()

        for good_version, good_hash, same_commit in [
            (True, True, True),
            (False, True, True),
            (True, False, True),
            (True, True, False),
        ]:
            with self.subTest(
                yanked=not good_version, hash=good_hash, commit=same_commit
            ):
                archive = crate(("d" if same_commit else "f") * 40)
                digest = hashlib.sha256(archive).hexdigest()
                idx = json.dumps(
                    {
                        "vers": "0.1.0",
                        "yanked": not good_version,
                        "cksum": digest if good_hash else "0" * 64,
                    }
                ).encode()
                with patch.object(release, "fetch", side_effect=[idx, archive]):
                    if good_version and good_hash and same_commit:
                        self.assertEqual(release.registry_pin(self.conversion), digest)
                    else:
                        with self.assertRaises(ValueError):
                            release.registry_pin(self.conversion)

    def test_build_rejects_tampered_artifact_before_extraction(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            crate = root / "candidate.crate"
            crate.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "checksum"):
                release.verify_archive(
                    crate, "0" * 64, root, root / "unpacked", "native"
                )
            self.assertFalse((root / "unpacked").exists())


if __name__ == "__main__":
    unittest.main()
