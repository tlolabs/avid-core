"""Give stripped macOS media tools content-derived UUIDs before final ad-hoc signing."""
import hashlib
import struct
import subprocess


def content_uuid(data):
    data = bytearray(data)
    if len(data) < 32 or struct.unpack_from('<I', data)[0] != 0xfeedfacf:
        raise ValueError('Expected a thin 64-bit little-endian Mach-O executable')
    count, size = struct.unpack_from('<II', data, 16)
    end = 32 + size
    if end > len(data):
        raise ValueError('Truncated Mach-O load commands')
    offset = 32
    uuid_offset = None
    for _ in range(count):
        if offset + 8 > end:
            raise ValueError('Truncated Mach-O load command')
        command, length = struct.unpack_from('<II', data, offset)
        if length < 8 or length % 8 or offset + length > end:
            raise ValueError('Invalid Mach-O load command size')
        if command == 0x1d:
            raise ValueError('Remove the code signature before deriving the UUID')
        if command == 0x1b:
            if length != 24 or uuid_offset is not None:
                raise ValueError('Invalid or duplicate Mach-O UUID')
            uuid_offset = offset + 8
        offset += length
    if offset != end or uuid_offset is None:
        raise ValueError('Missing UUID or inconsistent Mach-O load commands')
    data[uuid_offset:uuid_offset+16] = bytes(16)
    data[uuid_offset:uuid_offset+16] = hashlib.sha256(data).digest()[:16]
    return bytes(data)


def normalize(path):
    subprocess.run(['codesign', '--remove-signature', str(path)], check=True)
    path.write_bytes(content_uuid(path.read_bytes()))
    subprocess.run(['codesign', '--force', '--sign', '-', '--timestamp=none',
                    '--identifier', path.name, str(path)], check=True)
    subprocess.run(['codesign', '--verify', '--strict', str(path)], check=True)
