"""Platform-independent executable machine checks; native execution alone can hide emulation."""
import struct


def machine(data,target):
    arm=target.endswith('-arm64')
    if target.startswith('windows-'):
        if data[:2]!=b'MZ' or len(data)<64:raise ValueError('Expected PE executable')
        offset=struct.unpack_from('<I',data,60)[0]
        if offset+6>len(data) or data[offset:offset+4]!=b'PE\0\0':raise ValueError('Invalid PE header')
        actual=struct.unpack_from('<H',data,offset+4)[0];expected=0xaa64 if arm else 0x8664
    elif target.startswith('linux-'):
        if len(data)<20 or data[:6]!=b'\x7fELF\x02\x01':raise ValueError('Expected little-endian ELF64 executable')
        actual=struct.unpack_from('<H',data,18)[0];expected=183 if arm else 62
    elif target.startswith('macos-'):
        if len(data)<8 or data[:4]!=b'\xcf\xfa\xed\xfe':raise ValueError('Expected native Mach-O64 executable')
        actual=struct.unpack_from('<I',data,4)[0];expected=0x100000c if arm else 0x1000007
    else:raise ValueError('Unsupported executable target')
    if actual!=expected:raise ValueError('Executable architecture differs from target '+target)
