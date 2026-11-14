"""The distributed source must preserve build inputs and exclude player data."""
from pathlib import Path
import gzip
import io
import json
import hashlib
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import build_retrom_candidate as builder

class SourceTests(unittest.TestCase):
    def test_modified_extracted_source_updates_identity_and_keeps_omitted_rows(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / 'Cargo.lock'; source.write_text('original')
            rows = [dict(path='Cargo.lock', mode='100644', sha256=builder.digest(source)),
                    dict(path='src/raw/research.txt', mode='100644', sha256='a' * 64)]
            (root / '.retrom-source.json').write_text(json.dumps(dict(files=['Cargo.lock'], rows=rows,
                branch='feat/source', commit='b' * 40, dirty=False)))
            with patch.object(builder, 'ROOT', root):
                self.assertEqual(builder.git('status', '--porcelain'), '')
                original = builder.tree_digest(); source.write_text('modified')
                self.assertNotEqual(builder.tree_digest(), original)
                self.assertNotEqual(builder.git('status', '--porcelain'), '')
                builder.archive(root / 'modified.tar.gz', [])
                with tarfile.open(root / 'modified.tar.gz') as archive:
                    identity = json.load(archive.extractfile('ethornell/.retrom-source.json'))
                self.assertEqual(identity['rows'][0]['sha256'], builder.digest(source))
                self.assertEqual(identity['rows'][1], rows[1])
                self.assertTrue(identity['dirty'])
                actual = hashlib.sha256(json.dumps(identity['rows'], sort_keys=True,
                    separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
                self.assertEqual(actual, builder.tree_digest())

    def test_archive_is_complete_deterministic_and_excludes_user_data(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / '.git').mkdir()
            names = ['Cargo.lock', '.cargo/config.toml', 'rust-toolchain.toml',
                     'platform/wasm/audio-worklet.js', 'platform/retrom/bridge.mjs',
                     'tools/build_retrom_candidate.py', 'crates/core/src/lib.rs',
                     'src/raw/engine.asm', 'images/title.jpg', 'crates/ethornell-app/BGI0000.cad']
            for name in names:
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(name)
            git = lambda *args: {'branch': 'feat/source', 'rev-parse': 'a' * 40, 'status': ''}[args[0]]
            with patch.object(builder, 'ROOT', root), patch.object(builder, 'sources', return_value=names), patch.object(builder, 'git', side_effect=git):
                builder.archive(root / 'a.tar.gz', [])
                builder.archive(root / 'b.tar.gz', [])
            self.assertEqual((root / 'a.tar.gz').read_bytes(), (root / 'b.tar.gz').read_bytes())
            with tarfile.open(root / 'a.tar.gz') as archive:
                actual = set(archive.getnames())
                self.assertIn('ethornell/.cargo/config.toml', actual)
                self.assertIn('ethornell/rust-toolchain.toml', actual)
                self.assertNotIn('ethornell/src/raw/engine.asm', actual)
                self.assertNotIn('ethornell/images/title.jpg', actual)
                self.assertNotIn('ethornell/crates/ethornell-app/BGI0000.cad', actual)
                self.assertTrue(all(row.mtime == 0 and row.uid == 0 and row.gid == 0 for row in archive))

if __name__ == '__main__':
    unittest.main()
