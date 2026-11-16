#!/usr/bin/env python3
"""Inspect archive/object identities without executing target code or trusting filename suffixes."""
from pathlib import Path
import struct
import re

MACH_CPUS = {'arm64': 0x100000C, 'x86_64': 0x1000007}
ELF_CPUS = {'arm64': 183, 'x86_64': 62}
COFF_CPUS = {'arm64': 0xAA64, 'x86_64': 0x8664}
MACH_PLATFORMS = {'macos': 1, 'ios': 2, 'ios-simulator': 7}


def take(data, offset, length):
    if offset < 0 or length < 0 or offset + length > len(data):
        raise ValueError('Truncated native binary')
    return data[offset:offset + length]


def unpack(fmt, data, offset):
    return struct.unpack(fmt, take(data, offset, struct.calcsize(fmt)))


def members(data):
    offset = 8
    strings = b''
    while offset < len(data):
        header = take(data, offset, 60)
        if header[58:] != b'`\n':
            raise ValueError('Invalid static archive header')
        try:
            length = int(header[48:58])
        except ValueError as error:
            raise ValueError('Invalid static archive member size') from error
        payload = take(data, offset + 60, length)
        name = header[:16].decode('ascii').strip()
        if name.startswith('#1/'):
            name_size = int(name[3:])
            name = take(payload, 0, name_size).rstrip(b'\0').decode('utf-8')
            payload = payload[name_size:]
        elif name == '//':
            strings = payload
        elif name.startswith('/') and name[1:].isdigit():
            start = int(name[1:])
            name = strings[start:].split(b'/\n', 1)[0].decode('utf-8')
        if name not in ('/', '//', '/SYM64/') and not name.startswith('__.SYMDEF'):
            yield name.rstrip('/'), payload
        offset += 60 + length + length % 2
    if offset != len(data):
        raise ValueError('Invalid archive padding')


def macho(data, target):
    magic, cpu, _, filetype, commands, command_bytes, _, _ = unpack('<8I', data, 0)
    if magic != 0xFEEDFACF or cpu != MACH_CPUS[target['arch']]:
        raise ValueError('Mach-O architecture differs from target')
    if target['platform'] not in MACH_PLATFORMS:
        raise ValueError('Mach-O object in a non-Apple package')
    if filetype not in (1, 2, 6, 8):
        raise ValueError('Unsupported Mach-O file type')
    offset, platform, dependencies, search_paths, install_name = 32, None, [], [], None
    take(data, offset, command_bytes)
    for _ in range(commands):
        command, size = unpack('<II', data, offset)
        if size < 8 or offset + size > 32 + command_bytes:
            raise ValueError('Invalid Mach-O load command')
        if command == 0x32:
            platform = unpack('<I', data, offset + 8)[0]
        elif command == 0x24:
            platform = 1
        elif command == 0x25:
            # Older simulator x86 objects share this command with iOS devices;
            # the x86 architecture distinguishes their simulator target.
            platform = 7 if target['arch'] == 'x86_64' else 2
        if command == 0xD:
            name_offset = unpack('<I', data, offset + 8)[0]
            install_name = take(data, offset + name_offset, size - name_offset).split(b'\0', 1)[0].decode('utf-8')
            if not install_name.startswith(('@rpath/', '@loader_path/')) or '..' in Path(install_name).parts:
                raise ValueError('Mach-O install name contains an absolute/build path')
        if command == 0x8000001C:
            name_offset = unpack('<I', data, offset + 8)[0]
            path = take(data, offset + name_offset, size - name_offset).split(b'\0', 1)[0].decode('utf-8')
            if not path.startswith(('@loader_path', '@executable_path')) or '..' in Path(path).parts:
                raise ValueError('Mach-O dependency search path escapes package')
            search_paths.append(path)
        if command in (0xC, 0x18, 0x1F, 0x80000018, 0x8000001F, 0x80000023):
            name_offset = unpack('<I', data, offset + 8)[0]
            name = take(data, offset + name_offset, size - name_offset).split(b'\0', 1)[0].decode('utf-8')
            dependencies.append(name)
        offset += size
    if platform is not None and platform != MACH_PLATFORMS[target['platform']]:
        raise ValueError('Apple platform contamination (device/simulator/macOS)')
    if filetype == 6 and install_name is None:
        raise ValueError('Mach-O dylib install name is missing')
    for dependency in dependencies:
        if not dependency.startswith(('/System/Library/', '/usr/lib/', '@rpath/', '@loader_path/')):
            raise ValueError('Unpackaged/development machine Mach-O dependency: ' + dependency)
    return {'format': 'mach-o', 'arch': target['arch'], 'platform': platform, 'dependencies': dependencies, 'search_paths': search_paths, 'install_name': install_name}


def elf(data, target):
    if take(data, 4, 2) != b'\x02\x01':
        raise ValueError('Only little-endian ELF64 is supported')
    kind, machine = unpack('<HH', data, 16)
    if machine != ELF_CPUS[target['arch']] or target['platform'] not in ('linux', 'android'):
        raise ValueError('ELF target architecture/platform differs')
    if kind not in (1, 2, 3):
        raise ValueError('Unsupported ELF object type')
    phoff = unpack('<Q', data, 32)[0]
    phsize, phcount = unpack('<HH', data, 54)
    load_segments = []
    dynamic = None
    relro_segments = []
    writable_segments = []
    for i in range(phcount):
        fields = unpack('<IIQQQQQQ', data, phoff + i * phsize)
        type, flags, offset, vaddr, _, size, memory_size, align = fields
        if type == 1:
            load_segments.append((offset, vaddr, size, align))
            if flags & 2:
                writable_segments.append((vaddr, vaddr + memory_size))
            if target['platform'] == 'android' and kind in (2, 3):
                if align < 16384 or offset % 16384 != vaddr % 16384:
                    raise ValueError('Android ELF LOAD segment is not 16 KB page compatible')
        if type == 0x6474E552:
            relro_segments.append((vaddr, vaddr + memory_size))
        if type == 2:
            dynamic = (offset, size)
    if target['platform'] == 'android' and kind in (2, 3):
        for start, end in relro_segments:
            if end % 16384 and any(low <= start < end < high for low, high in writable_segments):
                raise ValueError('Android ELF RELRO shares an unaligned 16 KB page with writable data')
    dependencies, search_paths, glibc_versions = [], [], []
    if dynamic:
        entries = []
        for offset in range(dynamic[0], dynamic[0] + dynamic[1], 16):
            tag, value = unpack('<qQ', data, offset)
            if tag == 0:
                break
            entries.append((tag, value))
        string_vaddr = next((value for tag, value in entries if tag == 5), None)
        string_size = next((value for tag, value in entries if tag == 10), 0)
        string_offset = next((offset + string_vaddr - vaddr for offset, vaddr, size, _ in load_segments
                              if string_vaddr is not None and vaddr <= string_vaddr < vaddr + size), None)
        if string_offset is None:
            raise ValueError('ELF dynamic string table is missing')
        strings = take(data, string_offset, string_size)
        for tag, value in entries:
            if tag in (1, 15, 29):
                if value >= len(strings):
                    raise ValueError('ELF dynamic string offset is invalid')
                name = strings[value:].split(b'\0', 1)[0].decode('utf-8')
                if tag == 1:
                    if '/' in name or '\\' in name:
                        raise ValueError('ELF dependency contains an absolute/build path')
                    dependencies.append(name)
                else:
                    search_paths += name.split(':')
        for path in search_paths:
            if not path.startswith(('$ORIGIN', '${ORIGIN}')) or '..' in Path(path).parts:
                raise ValueError('ELF dependency search path escapes package')
        # Record the actual GLIBC symbol-version contract from DT_VERNEED,
        # rather than assigning the requested sysroot baseline to a newer host.
        need_vaddr = next((value for tag, value in entries if tag == 0x6FFFFFFE), None)
        need_count = next((value for tag, value in entries if tag == 0x6FFFFFFF), 0)
        if need_count > 1024:
            raise ValueError('ELF version requirement table is excessive')
        if need_count:
            need_offset = next((offset + need_vaddr - vaddr for offset, vaddr, size, _ in load_segments
                                if need_vaddr is not None and vaddr <= need_vaddr < vaddr + size), None)
            if need_offset is None:
                raise ValueError('ELF version requirement table is missing')
            for index in range(need_count):
                version, count, _, auxiliary, next_record = unpack('<HHIII', data, need_offset)
                if version != 1 or count > 1024:
                    raise ValueError('Invalid ELF version requirement record')
                auxiliary_offset = need_offset + auxiliary
                for item in range(count):
                    _, _, _, name_offset, next_auxiliary = unpack('<IHHII', data, auxiliary_offset)
                    if name_offset >= len(strings):
                        raise ValueError('Invalid ELF version name')
                    name = strings[name_offset:].split(b'\0', 1)[0].decode('utf-8')
                    if name.startswith('GLIBC_'):
                        if not re.fullmatch(r'GLIBC_[0-9]+(?:\.[0-9]+)+', name):
                            raise ValueError('Unsupported non-public or ABI-specific GLIBC symbol requirement: ' + name)
                        glibc_versions.append(name.removeprefix('GLIBC_'))
                    if item + 1 < count and not next_auxiliary:
                        raise ValueError('Truncated ELF version auxiliaries')
                    auxiliary_offset += next_auxiliary
                if index + 1 < need_count and not next_record:
                    raise ValueError('Truncated ELF version requirements')
                need_offset += next_record
    required_glibc = max(glibc_versions, key=lambda value: tuple(map(int, value.split('.'))), default=None)
    requested = re.fullmatch(r'glibc ([0-9]+(?:\.[0-9]+)+)', target.get('minimum', ''))
    if required_glibc and requested and tuple(map(int, required_glibc.split('.'))) > tuple(map(int, requested[1].split('.'))):
        raise ValueError('ELF requires GLIBC ' + required_glibc + ', newer than declared ' + requested[1] + '; rebuild with the target baseline sysroot')
    return {'format': 'elf', 'arch': target['arch'], 'dependencies': dependencies,
            'required_glibc': required_glibc, 'page_16k': target['platform'] == 'android' and kind in (2, 3), 'search_paths': search_paths}


def coff(data, target):
    if target['platform'] != 'windows':
        raise ValueError('COFF object in non-Windows package')
    if data[:2] == b'MZ':
        pe_offset = unpack('<I', data, 60)[0]
        if take(data, pe_offset, 4) != b'PE\0\0':
            raise ValueError('Invalid PE header')
        machine = unpack('<H', data, pe_offset + 4)[0]
    elif data[:4] == b'\0\0\xff\xff':
        machine = unpack('<H', data, 6)[0]
    else:
        machine = unpack('<H', data, 0)[0]
    if machine != COFF_CPUS[target['arch']]:
        raise ValueError('COFF architecture differs from target')
    return {'format': 'coff', 'arch': target['arch'], 'dependencies': []}


def validate(data, target):
    if data.startswith(b'!<arch>\n'):
        records = [validate(payload, target) for _, payload in members(data)]
        if not records:
            raise ValueError('Static archive has no object members')
        return {'format': 'archive', 'arch': target['arch'], 'members': len(records),
                'dependencies': sorted({name for record in records for name in record.get('dependencies', [])})}
    if data.startswith(b'\xcf\xfa\xed\xfe'):
        return macho(data, target)
    if data.startswith(b'\x7fELF'):
        return elf(data, target)
    if data.startswith(b'MZ') or data[:2] in (b'\x64\x86', b'\x64\xaa', b'\0\0'):
        return coff(data, target)
    raise ValueError('Unrecognized/unsupported native object (fat slices must be packaged separately)')


def validate_file(path, target):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or not path.stat().st_size:
        raise ValueError('Native artifact must be a nonempty regular file: ' + str(path))
    return validate(path.read_bytes(), target)
