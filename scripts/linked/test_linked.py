#!/usr/bin/env python3
"""Meaningful packaging regressions: target, page geometry, source and contamination rejection."""
import json
import os
import shutil
import subprocess
from pathlib import Path
import struct
import tempfile
import unittest

from binary import validate
from build import toolchain
from package import core_sources, inference_sdk, runtime_minimum
from rust_runtime import copy_rust_runtime, inspect_rust_runtime
from runtime import build_spec_projection, cxx_runtime, verify_inference_patch, verify_sdk_spec, verified_runtime_dependencies
from source import ROOT, SPEC_PATH, acquire, extract, signature


def target(platform='macos', arch='arm64'):
    spec = json.loads(SPEC_PATH.read_text())
    return next(t for t in spec['targets'] if t['platform'] == platform and t['arch'] == arch)


def macho(cpu=0x100000C, platform=1, dependency=None, install_name=None):
    commands = struct.pack('<6I', 0x32, 24, platform, 0xD0000, 0xD0000, 0)
    if dependency:
        name = dependency.encode() + b'\0'
        name += b'\0' * (-len(name) % 8)
        commands += struct.pack('<6I', 0xC, 24 + len(name), 24, 0, 0, 0) + name
    if install_name:
        name = install_name.encode() + b'\0'
        name += b'\0' * (-len(name) % 8)
        commands += struct.pack('<6I', 0xD, 24 + len(name), 24, 0, 0, 0) + name
    return struct.pack('<8I', 0xFEEDFACF, cpu, 0, 6 if install_name else 1, 1 + bool(dependency) + bool(install_name), len(commands), 0, 0) + commands


def elf(align=16384, offset=0, vaddr=0):
    header = bytearray(64)
    header[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<HH', header, 16, 3, 183)
    struct.pack_into('<Q', header, 32, 64)
    struct.pack_into('<HH', header, 54, 56, 1)
    return bytes(header) + struct.pack('<IIQQQQQQ', 1, 5, offset, vaddr, 0, 120, 120, align)



def elf_glibc(version):
    data = bytearray(1024)
    data[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<HH', data, 16, 3, 183)
    struct.pack_into('<Q', data, 32, 64)
    struct.pack_into('<HH', data, 54, 56, 2)
    struct.pack_into('<IIQQQQQQ', data, 64, 1, 5, 0, 0, 0, 1024, 1024, 4096)
    struct.pack_into('<IIQQQQQQ', data, 120, 2, 4, 0x100, 0x100, 0, 96, 96, 8)
    strings = b'\0libc.so.6\0' + ('GLIBC_' + version).encode() + b'\0'
    data[0x200:0x200 + len(strings)] = strings
    entries = [(5, 0x200), (10, len(strings)), (0x6FFFFFFE, 0x280), (0x6FFFFFFF, 1), (1, 1), (0, 0)]
    for index, entry in enumerate(entries): struct.pack_into('<qQ', data, 0x100 + index * 16, *entry)
    struct.pack_into('<HHIII', data, 0x280, 1, 1, 1, 16, 0)
    struct.pack_into('<IHHII', data, 0x290, 0, 0, 2, len(b'\0libc.so.6\0'), 0)
    return bytes(data)

def archive(objects):
    result = b'!<arch>\n'
    for i, data in enumerate(objects):
        header = ('part%d.o/' % i).ljust(16) + '0'.ljust(12) + '0'.ljust(6) + '0'.ljust(6) + '100644'.ljust(8) + str(len(data)).ljust(10) + '`\n'
        result += header.encode() + data + (b'\n' if len(data) % 2 else b'')
    return result


class BinaryTests(unittest.TestCase):
    def test_every_archive_member_must_match(self):
        self.assertEqual(validate(archive([macho(), macho()]), target())['members'], 2)
        with self.assertRaisesRegex(ValueError, 'architecture'):
            validate(archive([macho(), macho(cpu=0x1000007)]), target())

    def test_apple_device_simulator_macos_are_distinct(self):
        for platform_id, system in [(1, 'macos'), (2, 'ios'), (7, 'ios-simulator')]:
            validate(macho(platform=platform_id), target(system))
            for other in {'macos', 'ios', 'ios-simulator'} - {system}:
                with self.assertRaisesRegex(ValueError, 'platform contamination'):
                    validate(macho(platform=platform_id), target(other))

    def test_dynamic_paths_cannot_reference_build_machine(self):
        validate(macho(dependency='/System/Library/Frameworks/VideoToolbox.framework/Versions/A/VideoToolbox'), target())
        with self.assertRaisesRegex(ValueError, 'development machine'):
            validate(macho(dependency='/opt/homebrew/opt/ffmpeg/lib/libavcodec.dylib'), target())

    def test_dylib_install_identity_must_be_relocatable(self):
        self.assertEqual(validate(macho(install_name='@rpath/libavid_core_ffi.dylib'), target())['install_name'], '@rpath/libavid_core_ffi.dylib')
        for path in ('/Users/developer/target/libavid_core_ffi.dylib', '/tmp/libavid_core_ffi.dylib', '@rpath/../libavid_core_ffi.dylib'):
            with self.assertRaisesRegex(ValueError, 'install name'):
                validate(macho(install_name=path), target())

    def test_android_load_segments_require_16k_alignment(self):
        self.assertTrue(validate(elf(), target('android'))['page_16k'])
        for data in (elf(4096), elf(16384, offset=4096)):
            with self.assertRaisesRegex(ValueError, '16 KB'):
                validate(data, target('android'))


    def test_android_relro_cannot_protect_following_writable_data(self):
        data = bytearray(elf())
        struct.pack_into('<H', data, 56, 2)
        # Writable LOAD spans 0..0x8000; RELRO ends halfway through a 16K page.
        struct.pack_into('<IIQQQQQQ', data, 64, 1, 6, 0, 0, 0, 176, 0x8000, 16384)
        data += struct.pack('<IIQQQQQQ', 0x6474E552, 4, 0, 0, 0, 0x1000, 0x1000, 1)
        with self.assertRaisesRegex(ValueError, 'RELRO'):
            validate(bytes(data), target('android'))
        struct.pack_into('<Q', data, 120 + 40, 0x4000)
        validate(bytes(data), target('android'))

    def test_actual_glibc_symbol_versions_cannot_exceed_claimed_baseline(self):
        self.assertEqual(validate(elf_glibc('2.28'), target('linux'))['required_glibc'], '2.28')
        with self.assertRaisesRegex(ValueError, 'newer than declared'):
            validate(elf_glibc('2.39'), target('linux'))
        newer = dict(target('linux'), minimum='glibc 2.39')
        self.assertEqual(validate(elf_glibc('2.39'), newer)['required_glibc'], '2.39')

    def test_malformed_objects_do_not_pass(self):
        for data in (b'', b'not an object', archive([]), b'!<arch>\n' + b'x' * 60, macho()[:-1]):
            with self.assertRaises(ValueError):
                validate(data, target())

    def test_archive_coff_and_elf_wrong_target_rejected(self):
        coff = struct.pack('<H', 0xAA64) + b'\0' * 18
        validate(archive([coff]), target('windows'))
        with self.assertRaises(ValueError):
            validate(archive([coff]), target('linux'))
        with self.assertRaises(ValueError):
            validate(elf(), target('macos'))


class ContractTests(unittest.TestCase):
    def test_pinned_spec_is_independent_from_historical_recipe(self):
        spec = json.loads(SPEC_PATH.read_text())
        old = json.loads((ROOT / 'runtime/ffmpeg/spec.json').read_text())
        self.assertEqual(spec['ffmpeg'], old['source'])
        self.assertEqual(len({t['rust_target'] for t in spec['targets']}), 11)
        self.assertIn('--disable-programs', spec['configure'])
        self.assertIn('--disable-network', spec['configure'])
        self.assertIn('--enable-protocol=file', spec['configure'])
        self.assertEqual(spec['profiles']['apple-lgpl']['dependencies'], ['zlib'])
        self.assertIn('--disable-gpl', spec['profiles']['apple-lgpl']['configure'])
        self.assertIn('libx264', spec['profiles']['gpl-software']['encoders'])

    def test_offline_source_rejects_hash_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            (cache / 'source.tar.gz').write_bytes(b'bad')
            with self.assertRaisesRegex(ValueError, 'Missing or invalid pinned'):
                acquire('source', {'url': 'https://example.invalid/source.tar.gz', 'sha256': '0' * 64}, cache)

    def test_signature_requires_exact_single_fingerprint(self):
        signature('[GNUPG:] VALIDSIG ABC other fields', 'ABC')
        for status in ('', '[GNUPG:] VALIDSIG WRONG other', '[GNUPG:] VALIDSIG ABC other\n[GNUPG:] VALIDSIG ABC other', '[GNUPG:] BADSIG bad\n[GNUPG:] VALIDSIG ABC other'):
            with self.assertRaises(ValueError):
                signature(status, 'ABC')

    def test_corresponding_source_keeps_inference_recipe_and_legal_docs(self):
        sources = core_sources()
        for name in ('script/build_whisper.py', 'docs/linked-artifacts.md', 'LICENSING_REVIEW.md', 'docs/BUILDING.md'):
            self.assertIn(name, sources)

    def test_combined_inference_minimum_cannot_claim_media_only_baseline(self):
        self.assertEqual(runtime_minimum(target(), None)['combined'], '13.0')
        sdk = {'metadata': {'configuration': ['-DCMAKE_OSX_DEPLOYMENT_TARGET=14.0']}}
        self.assertEqual(runtime_minimum(target(), sdk)['combined'], '14.0')
        sdk = {'metadata': {'configuration': ['-DANDROID_PLATFORM=android-28']}}
        self.assertEqual(runtime_minimum(target('android'), sdk)['combined'], '28')

    def test_inference_packaging_requires_exact_runtime_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            prefix = Path(temporary)
            (prefix / 'avid-whisper-build.json').write_text(json.dumps({'schema': 1, 'kind': 'avid-linked-inference', 'target': 'wrong'}))
            with self.assertRaisesRegex(ValueError, 'pinned runtime/target'):
                inference_sdk(prefix, target())

    def test_inference_requires_exact_exception_patch_and_retained_rights(self):
        directory = ROOT / 'licenses/linked-inference/patches'
        spec = json.loads((directory / 'manifest.json').read_text())
        metadata = {'sdk_variant': spec['variant'], 'source_patches': [spec],
                    'sdk_license': 'MIT AND GPL-3.0-or-later'}
        with tempfile.TemporaryDirectory() as temporary:
            prefix = Path(temporary)
            notices = prefix / 'licenses'
            notices.mkdir()
            for name, source in ((spec['patch'], directory / spec['patch']),
                                 ('AVID-EXCEPTION-PATCH-GPL-3.0.txt', ROOT / 'LICENSE'),
                                 ('whisper-MIT.txt', ROOT / 'licenses/linked-inference/whisper.cpp-MIT.txt')):
                shutil.copy2(source, notices / name)
            (notices / 'patch-manifest.json').write_text(json.dumps(spec))
            verify_inference_patch(prefix, metadata)
            for key, value in (('sdk_variant', 'unpatched'), ('source_patches', []), ('sdk_license', 'MIT')):
                with self.assertRaisesRegex(ValueError, 'patch identity/license'):
                    verify_inference_patch(prefix, {**metadata, key: value})
            patch = notices / spec['patch']
            patch.write_bytes(patch.read_bytes() + b'changed')
            with self.assertRaisesRegex(ValueError, 'changed patch source/license'):
                verify_inference_patch(prefix, metadata)
            shutil.copy2(directory / spec['patch'], patch)
            (notices / 'AVID-EXCEPTION-PATCH-GPL-3.0.txt').unlink()
            with self.assertRaisesRegex(ValueError, 'changed patch source/license'):
                verify_inference_patch(prefix, metadata)

    def test_android_requires_pinned_ndk_not_host_compiler(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with self.assertRaisesRegex(ValueError, '--ndk is required'):
                toolchain(target('android'), directory, None)
            (directory / 'source.properties').write_text('Pkg.Revision = 1.0\n')
            with self.assertRaisesRegex(ValueError, 'pinned Android NDK'):
                toolchain(target('android'), directory, directory)



class NativeLinkPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix='avid-link-policy-')
        cls.directory = Path(cls.temporary.name)
        source = cls.directory / 'policy.rs'
        source.write_text(
            '#[path=' + json.dumps(str(ROOT / 'crates/avid-media-bridge/link_order.rs')) + '] mod link_order;\n'
            '#[path=' + json.dumps(str(ROOT / 'crates/avid-whisper-bridge/link_runtime.rs')) + '] mod link_runtime;\n'
            'fn main() { let a:Vec<String>=std::env::args().skip(1).collect(); '
            'if a[0]=="order" { let groups:Vec<Vec<String>>=a[1..].iter().map(|s|s.split(",").map(str::to_owned).collect()).collect(); '
            'println!("{}",link_order::libraries_in_link_order(&groups).join(",")); } '
            'else { match link_runtime::cpp_runtime(&a[1],&a[2],a.get(3).map(String::as_str)) {'
            'Ok(v)=>println!("{}",v.unwrap_or_default()),Err(e)=>{eprintln!("{e}");std::process::exit(2);}} }}')
        cls.executable = cls.directory / ('policy.exe' if os.name == 'nt' else 'policy')
        subprocess.run(['rustc', '--edition=2021', str(source), '-o', str(cls.executable)], check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_windows_gnu_and_llvm_emit_the_declared_cpp_runtime(self):
        for runtime in ('c++', 'stdc++'):
            output = subprocess.check_output([self.executable, 'runtime', 'windows', 'gnu', runtime], text=True).strip()
            self.assertEqual(output, runtime)
        p = subprocess.run([self.executable, 'runtime', 'windows', 'gnu'], capture_output=True)
        self.assertEqual(p.returncode, 2)
        self.assertEqual(subprocess.check_output([self.executable, 'runtime', 'windows', 'msvc'], text=True).strip(), '')
        self.assertEqual(subprocess.check_output([self.executable, 'runtime', 'android', '', 'c++_shared'], text=True).strip(), 'c++_shared')
        self.assertEqual(subprocess.run([self.executable, 'runtime', 'android', '', 'c++_static'], capture_output=True).returncode, 2)

    def test_dependency_order_links_real_archives_with_single_pass_semantics(self):
        clang = shutil.which('clang')
        ar = shutil.which('llvm-ar') or ('/opt/homebrew/opt/llvm/bin/llvm-ar' if Path('/opt/homebrew/opt/llvm/bin/llvm-ar').is_file() else None)
        lld = shutil.which('ld.lld')
        if not lld:
            sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
            host = next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
            candidate = sysroot / 'lib/rustlib' / host / 'bin/gcc-ld' / ('ld.lld.exe' if os.name == 'nt' else 'ld.lld')
            if candidate.is_file(): lld = str(candidate)
        if not all((clang, ar, lld)):
            self.skipTest('ELF clang, LLVM ar and LLD required for actual single-pass link regression')
        groups = ['avformat,avcodec,swresample,avutil,m', 'avcodec,swresample,avutil,m',
                  'avfilter,swscale,swresample,avutil,m', 'avutil,m', 'swscale,avutil,m', 'swresample,avutil,m']
        order = subprocess.check_output([self.executable, 'order', *groups], text=True).strip().split(',')
        old_order = list(dict.fromkeys(','.join(groups).split(',')))
        sources = {
            'entry': 'extern int format(void),filter(void); volatile int result; void _start(void){result=format()+filter();for(;;){}}',
            'format': 'extern int codec(void);int format(void){return codec();}',
            'codec': 'extern int util_a(void);int codec(void){return util_a();}',
            'filter': 'extern int scale(void);int filter(void){return scale();}',
            'scale': 'extern int util_b(void);int scale(void){return util_b();}',
            'resample': 'int resample(void){return 0;}',
            'util_a': 'int util_a(void){return 1;}',
            'util_b': 'extern int math_value(void);int util_b(void){return math_value();}',
            'math': 'int math_value(void){return 2;}'}
        objects = {}
        for name, text in sources.items():
            path = self.directory / (name + '.c'); path.write_text(text)
            object_path = path.with_suffix('.o')
            subprocess.run([clang, '--target=x86_64-unknown-linux-gnu', '-ffreestanding', '-fno-stack-protector', '-c', str(path), '-o', str(object_path)], check=True, capture_output=True)
            objects[name] = object_path
        archive_members = {'avformat': ['format'], 'avcodec': ['codec'], 'avfilter': ['filter'],
                           'swscale': ['scale'], 'swresample': ['resample'], 'avutil': ['util_a', 'util_b'], 'm': ['math']}
        for name, values in archive_members.items():
            subprocess.run([ar, 'rcs', str(self.directory / ('lib' + name + '.a')), *[str(objects[v]) for v in values]], check=True, capture_output=True)
        # LLD normally permits backreferences; fatal backreference diagnostics
        # enforce GNU ld's one-pass archive rules and expose the original bug.
        command = [lld, '--warn-backrefs', '--fatal-warnings', '-m', 'elf_x86_64', '-e', '_start', str(objects['entry']), '-L' + str(self.directory)]
        old = subprocess.run(command + ['-l' + name for name in old_order] + ['-o', str(self.directory / 'old.elf')], capture_output=True, text=True)
        self.assertNotEqual(old.returncode, 0, old.stderr)
        self.assertIn('backward reference', old.stderr)
        new = subprocess.run(command + ['-l' + name for name in order] + ['-o', str(self.directory / 'linked.elf')], capture_output=True, text=True)
        self.assertEqual(new.returncode, 0, new.stderr)
        self.assertTrue((self.directory / 'linked.elf').read_bytes().startswith(b'\x7fELF'))


class RuntimeOwnershipTests(unittest.TestCase):
    def test_android_cpp_is_not_a_system_or_static_runtime(self):
        self.assertEqual(cxx_runtime(target('android')), 'c++_shared')
        with self.assertRaises(ValueError): cxx_runtime(target('android'), 'stdc++')
        with self.assertRaises(ValueError): cxx_runtime(target('windows'))
        with tempfile.TemporaryDirectory() as temporary:
            prefix = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'single shared'):
                verified_runtime_dependencies(prefix, {}, target('android'))
            path = prefix / 'runtime/lib/libc++_shared.so'; path.parent.mkdir(parents=True); path.write_bytes(elf())
            notice = prefix / 'runtime/licenses/NOTICE'; notice.parent.mkdir(parents=True); notice.write_text('NDK notice')
            from source import digest
            metadata = {'cxx_runtime': 'c++_shared', 'runtime_dependencies': {'libc++_shared.so': {
                'path': 'runtime/lib/libc++_shared.so', 'sha256': digest(path), 'ndk_revision': '28.2.13676358',
                'notices': {'runtime/licenses/NOTICE': digest(notice)}}}}
            self.assertEqual(verified_runtime_dependencies(prefix, metadata, target('android')), {'libc++_shared.so': path.resolve()})
            path.write_bytes(elf(4096))
            with self.assertRaises(ValueError): verified_runtime_dependencies(prefix, metadata, target('android'))

    def test_added_profile_does_not_relabel_an_existing_sdk_recipe(self):
        spec = json.loads(SPEC_PATH.read_text())
        self.assertEqual(spec['profiles']['apple-gpl']['default_encoding'], 'automatic')
        self.assertIn('libx264', spec['profiles']['apple-gpl']['encoders'])
        self.assertIn('h264_videotoolbox', spec['profiles']['apple-gpl']['encoders'])
        old = json.loads(json.dumps(spec)); del old['profiles']['apple-gpl']
        self.assertEqual(build_spec_projection(old, 'macos-arm64', 'apple-lgpl'), build_spec_projection(spec, 'macos-arm64', 'apple-lgpl'))
        changed = json.loads(json.dumps(spec)); changed['profiles']['apple-lgpl']['configure'].append('--enable-gpl')
        self.assertNotEqual(build_spec_projection(old, 'macos-arm64', 'apple-lgpl'), build_spec_projection(changed, 'macos-arm64', 'apple-lgpl'))



class RustRuntimeNoticeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='avid-rust-notices-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.target = 'x86_64-unknown-linux-gnu'
        self.libraries = self.root / 'lib/rustlib' / self.target / 'lib'
        self.libraries.mkdir(parents=True)
        for name in ('std', 'core', 'alloc', 'compiler_builtins'):
            (self.libraries / ('lib' + name + '-test.rlib')).write_bytes(('runtime ' + name).encode())
        (self.root / 'lib/rustlib' / ('manifest-rust-std-' + self.target)).write_text('exact target install manifest')
        self.notices = self.root / 'share/doc/rust'
        (self.notices / 'licenses').mkdir(parents=True)
        for name in ('COPYRIGHT-library.html', 'COPYRIGHT.html'):
            (self.notices / name).write_text('original upstream ' + name)
        for name in ('MIT.txt', 'Apache-2.0.txt', 'LLVM-exception.txt'):
            (self.notices / 'licenses' / name).write_text('original upstream ' + name)
        self.identity = 'rustc 1.99.1 test identity'
        self.record = inspect_rust_runtime(self.target, self.root, self.identity)

    def test_exact_runtime_notices_are_copied_with_component_inventory(self):
        output = self.root / 'packaged'
        copy_rust_runtime(self.record, output, self.root, self.identity)
        for relative, expected in self.record['notices'].items():
            from source import digest
            copied = output / Path(relative).relative_to('share/doc/rust')
            self.assertEqual(digest(copied), expected)
        inventory = json.loads((output / 'runtime-inventory.json').read_text())
        self.assertIn('libcompiler_builtins-test.rlib', inventory['available_target_runtime_libraries'])
        self.assertEqual(inventory['rustc'], self.identity)
        self.assertEqual((output / 'rust-std-install-manifest.txt').read_text(), 'exact target install manifest')

    def test_missing_or_changed_toolchain_notices_do_not_package(self):
        notice = self.notices / 'licenses/MIT.txt'
        notice.write_text('changed attribution')
        with self.assertRaisesRegex(ValueError, 'differ from'):
            copy_rust_runtime(self.record, self.root / 'changed', self.root, self.identity)
        self.assertFalse((self.root / 'changed').exists())
        notice.unlink()
        with self.assertRaisesRegex(ValueError, 'license text'):
            inspect_rust_runtime(self.target, self.root, self.identity)

    def test_different_std_builtin_or_compiler_identity_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'differ from'):
            copy_rust_runtime(self.record, self.root / 'compiler-changed', self.root, 'different rust compiler')
        (self.libraries / 'libcompiler_builtins-test.rlib').write_bytes(b'different compiler builtins')
        with self.assertRaisesRegex(ValueError, 'differ from'):
            copy_rust_runtime(self.record, self.root / 'runtime-changed', self.root, self.identity)
        (self.libraries / 'libcompiler_builtins-test.rlib').unlink()
        with self.assertRaisesRegex(ValueError, 'lacks compiler_builtins'):
            inspect_rust_runtime(self.target, self.root, self.identity)


if __name__ == '__main__':
    unittest.main()
