"""Canonical recipes for the hardware interfaces established by native baseline inventories."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
from build import run


def native(path, system):
    if system == 'windows':
        return subprocess.check_output(['cygpath','-m',str(path)],text=True).strip()
    return str(path)


def build_hardware(deps, prefix, work, target, env, jobs):
    system = target['os']
    flags = []
    if 'ffnvcodec' in deps:
        run(['make','PREFIX='+str(prefix),'install'],deps['ffnvcodec'],env)
        flags += ['--enable-ffnvcodec','--enable-nvenc']
    if 'amf' in deps:
        shutil.copytree(deps['amf']/'amf/public/include',prefix/'include/AMF')
        flags += ['--enable-amf']
    def cmake(name, options):
        dest=work/(name+'-build')
        run(['cmake','-S',deps[name],'-B',dest,'-G','MSYS Makefiles' if system=='windows' else 'Unix Makefiles',
             '-DCMAKE_BUILD_TYPE=Release','-DCMAKE_INSTALL_PREFIX='+str(prefix),'-DCMAKE_INSTALL_LIBDIR=lib',
             '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',*options],work,env)
        run(['cmake','--build',dest,'--parallel',jobs],work,env)
        run(['cmake','--install',dest],work,env)
    if 'meson' in deps:
        cmake('ninja',['-DBUILD_TESTING=OFF'])
        python = os.environ.get('AVID_NATIVE_PYTHON',sys.executable)
        meson = [python,native(deps['meson']/'meson.py',system)]
        menv = dict(env)
        if system == 'windows':
            menv['PKG_CONFIG_LIBDIR']=native(prefix/'lib/pkgconfig',system)
        def meson_build(name, options, library='static', sdk=False):
            dest=work/(name+'-build')
            benv=dict(menv)
            if sdk:
                # Only this temporary ABI-reference library uses platform X11 SDK headers.
                benv['PKG_CONFIG_LIBDIR'] += ':/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig'
            run([*meson,'setup',native(dest,system),native(deps[name],system),
                 '--prefix='+native(prefix,system),'--libdir=lib','--buildtype=release',
                 '--default-library='+library,'--wrap-mode=nodownload',*options],work,benv)
            run([*meson,'compile','-C',native(dest,system),'-j',jobs],work,benv)
            run([*meson,'install','-C',native(dest,system)],work,benv)
        if 'libdrm' in deps:
            meson_build('libdrm',['-D'+n+'=disabled' for n in ['intel','radeon','amdgpu','nouveau','vmwgfx','omap','exynos','freedreno','tegra','vc4','etnaviv','cairo-tests','man-pages','valgrind']]+['-Dtests=false'])
        # Upstream hardcodes shared_library. Honor Meson's library selection without
        # modifying APIs or runtime policy. The exact patch is included in the recipe.
        p=deps['libva']/'va/meson.build'
        text=p.read_text()
        if text.count('shared_library(') != 6:
            raise ValueError('Pinned libva static-selection patch no longer applies')
        p.write_text(text.replace('shared_library(', 'library('))
        options=['-Denable_docs=false','-Dwith_glx=no','-Dwith_wayland=no']
        if system=='windows':
            meson_build('libva',options+['-Dwith_x11=no','-Dwith_win32=yes'])
        else:
            meson_build('libva',options+['-Dwith_x11=yes','-Dwith_win32=no','--sysconfdir=/etc',
                        '-Ddriverdir=/usr/lib/x86_64-linux-gnu/dri'],library='shared',sdk=True)
            # Match the former runtime's lazy loading: software-only installations
            # do not gain an unconditional libva/X11 dependency at program startup.
            for stem in ['va','va-drm','va-x11']:
                shim=work/(stem+'-shim');shim.mkdir()
                soname='lib'+stem+'.so.2'
                run([sys.executable,deps['implib']/'implib-gen.py','--target','x86_64-linux-gnu',
                     '--dlopen','--lazy-load',prefix/'lib'/soname],shim,env)
                files=sorted(shim.glob('*.tramp.S'))+sorted(shim.glob('*.init.c'))
                run([env['CC'],*env['CFLAGS'].split(),'-fPIC','-Wa,--noexecstack','-DIMPLIB_HIDDEN_SHIMS','-c',*files],shim,env)
                run(['ar','rcs',prefix/'lib'/('lib'+stem+'.a'),*sorted(shim.glob('*.o'))],shim,env)
                pc=prefix/'lib/pkgconfig'/('lib'+stem+'.pc')
                lines=[l for l in pc.read_text().splitlines() if not l.startswith(('Libs:','Libs.private:','Requires:','Requires.private:'))]
                pc.write_text('\n'.join(lines)+f'\nLibs: -L${{libdir}} -l{stem}\nLibs.private: -ldl\n')
            for p in (prefix/'lib').glob('libva*.so*'):
                p.unlink()
        flags += ['--enable-vaapi']
    if 'libvpl' in deps:
        cmake('libvpl',['-DBUILD_SHARED_LIBS=OFF','-DBUILD_TESTS=OFF','-DBUILD_EXAMPLES=OFF',
                        '-DBUILD_EXPERIMENTAL=OFF','-DENABLE_LIBDIR_IN_RUNTIME_SEARCH=OFF',
                        '-DMFX_MODULES_DIR=/usr/lib/x86_64-linux-gnu'])
        pc=prefix/'lib/pkgconfig/vpl.pc'
        # Static C++ dispatch library consumed by FFmpeg's C linker.
        with pc.open('a') as f:
            f.write('\nLibs.private: -l'+('c++' if system=='windows' else 'stdc++')+'\n')
        flags += ['--enable-libvpl']
    return flags
