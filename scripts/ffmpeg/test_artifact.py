import io
import struct
from binary import machine
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from artifact import archive_files, validate_payload
from acquire import acquire
from provenance import tree, valid_signature


class ArchiveTrustTests(unittest.TestCase):
    def archive(self, name, kind=tarfile.REGTYPE, duplicate=False):
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w') as tar:
            for _ in range(2 if duplicate else 1):
                item=tarfile.TarInfo(name);item.type=kind;item.linkname='/outside'
                tar.addfile(item,io.BytesIO())
        return stream.getvalue()

    def test_unsafe_members_are_rejected_before_extraction(self):
        for name,kind,duplicate in [('../escape',tarfile.REGTYPE,False),('/escape',tarfile.REGTYPE,False),
                                    ('C:/escape',tarfile.REGTYPE,False),('safe',tarfile.SYMTYPE,False),
                                    ('safe',tarfile.REGTYPE,True)]:
            with self.subTest(name=name,kind=kind), tempfile.TemporaryDirectory() as d:
                p=Path(d)/'bad.tar';p.write_bytes(self.archive(name,kind,duplicate))
                with self.assertRaises(ValueError):archive_files(p)

    def test_checksums_cannot_substitute_for_required_evidence(self):
        with self.assertRaisesRegex(ValueError,'Missing runtime evidence'):
            validate_payload({}, {}, 'macos-arm64')

    def test_candidate_acquisition_never_contacts_network_or_changes_destination(self):
        with tempfile.TemporaryDirectory() as d, patch('acquire.gh') as network:
            destination=Path(d)/'working';destination.mkdir();(destination/'ffmpeg').write_bytes(b'working')
            with self.assertRaisesRegex(ValueError,'qualification is incomplete'):
                acquire('macos-arm64',destination)
            self.assertEqual((destination/'ffmpeg').read_bytes(),b'working')
            network.assert_not_called()

    def test_wrong_architecture_is_rejected_even_when_emulation_can_execute_it(self):
        data=bytearray(128);data[:2]=b'MZ';struct.pack_into('<I',data,60,64);data[64:68]=b'PE\0\0';struct.pack_into('<H',data,68,0x8664)
        machine(data,'windows-x86_64')
        with self.assertRaises(ValueError):machine(data,'windows-arm64')

    def test_pinned_signature_fingerprint_is_exact(self):
        valid_signature('[GNUPG:] VALIDSIG ABC 0','ABC')
        with self.assertRaises(ValueError):valid_signature('[GNUPG:] VALIDSIG OTHER 0','ABC')
        with self.assertRaises(ValueError):valid_signature('[GNUPG:] GOODSIG ABC 0','ABC')
        with self.assertRaises(ValueError):valid_signature('[GNUPG:] EXPKEYSIG ABC 0\n[GNUPG:] VALIDSIG ABC 0','ABC')

    def test_source_tree_comparison_includes_executable_bits(self):
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w') as tar:
            item=tarfile.TarInfo('configure');item.mode=0o755;tar.addfile(item,io.BytesIO())
        self.assertTrue(tree(stream.getvalue())['configure']['executable'])

if __name__=='__main__':unittest.main()
