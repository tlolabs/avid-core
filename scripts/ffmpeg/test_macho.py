import struct
import unittest
from macho import content_uuid


def fixture(uuid=b'a'*16, text=b'code'):
    return struct.pack('<8I', 0xfeedfacf, 0x0100000c, 0, 2, 1, 24, 0, 0) + struct.pack('<II', 0x1b, 24) + uuid + text


class ContentUuidTests(unittest.TestCase):
    def test_uuid_ignores_old_identity_but_covers_executable_contents(self):
        first = content_uuid(fixture())
        self.assertEqual(first, content_uuid(fixture(b'b'*16)))
        self.assertEqual(first, content_uuid(first))
        self.assertNotEqual(first[40:56], content_uuid(fixture(text=b'changed'))[40:56])
        self.assertEqual(first[56:], b'code')

    def test_malformed_or_signed_inputs_are_rejected(self):
        signed = bytearray(fixture())
        struct.pack_into('<I', signed, 32, 0x1d)
        bad_size = bytearray(fixture())
        struct.pack_into('<I', bad_size, 36, 0)
        for data in [b'', fixture()[:48], signed, bad_size]:
            with self.assertRaises(ValueError):
                content_uuid(data)
