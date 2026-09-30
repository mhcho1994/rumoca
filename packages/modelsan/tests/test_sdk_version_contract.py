"""The SDK and the compiler read one bitcode version, the same one.

docs/SPEC_RUMOCA_BITCODE.md §4.1: every reader implements exactly one
``RBC_VERSION`` and refuses any other before it reads the payload.
"""
from pathlib import Path
import re
import unittest

import rumoca_bitcode
from rumoca_bitcode import BitcodeError, Model

ROOT = Path(__file__).resolve().parents[3]


class VersionContract(unittest.TestCase):
    def test_the_sdk_reads_the_version_the_compiler_writes(self):
        schema = (ROOT / "crates/rumoca-bitcode/src/schema.rs").read_text()
        (compiler,) = re.findall(r"pub const RBC_VERSION: u32 = (\d+);", schema)
        self.assertEqual(rumoca_bitcode.VERSION, int(compiler))

    def test_another_version_is_refused_before_the_payload_is_read(self):
        document = Model.empty("Empty")._document
        for version in (rumoca_bitcode.VERSION - 1, rumoca_bitcode.VERSION + 1):
            with self.subTest(version=version):
                # The payload is garbage: a reader that looked at it first
                # would fail differently.
                with self.assertRaisesRegex(BitcodeError, "unsupported bitcode version"):
                    Model({**document, "bitcode_version": version, "model": None})

    def test_the_sdk_release_version_is_separate(self):
        pyproject = (ROOT / "packages/rumoca-bitcode/pyproject.toml").read_text()
        self.assertIn(f'version = "{rumoca_bitcode.__version__}"', pyproject)


if __name__ == "__main__":
    unittest.main()
