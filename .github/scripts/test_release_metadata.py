"""Exercise publication guards without creating tags or contacting a registry."""

from pathlib import Path
import tempfile
import unittest

from release_metadata import metadata


class ReleaseMetadataTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.manifest = Path(directory.name) / "Cargo.toml"
        self.manifest.write_text('[package]\nname = "md5-simd"\nversion = "0.1.0"\n')

    def test_matching_tag(self):
        self.assertEqual(metadata(self.manifest, "tag", "v0.1.0"), ("md5-simd", "0.1.0"))

    def test_prerelease_tag(self):
        self.manifest.write_text('[package]\nname = "md5-simd"\nversion = "0.2.0-rc.1"\n')
        self.assertEqual(metadata(self.manifest, "tag", "v0.2.0-rc.1")[1], "0.2.0-rc.1")

    def test_branch_dispatch_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "select a version tag"):
            metadata(self.manifest, "branch", "v0.1.0")

    def test_wrong_or_unprefixed_version_is_rejected(self):
        for tag in ["v0.2.0", "0.1.0", "main", "v0.1.0\ninjected=true"]:
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "does not match"):
                metadata(self.manifest, "tag", tag)

    def test_publication_disabled_is_rejected(self):
        with self.manifest.open("a") as stream:
            stream.write("publish = false\n")
        with self.assertRaisesRegex(ValueError, "disables registry publication"):
            metadata(self.manifest, "tag", "v0.1.0")

    def test_multiline_output_is_rejected(self):
        self.manifest.write_text('[package]\nname = "md5-simd\\ninjected=true"\nversion = "0.1.0"\n')
        with self.assertRaisesRegex(ValueError, "single-line"):
            metadata(self.manifest, "tag", "v0.1.0")


if __name__ == "__main__":
    unittest.main()
