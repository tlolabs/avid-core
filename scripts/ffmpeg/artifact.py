"""Read-only archive contract shared by release promotion and host acquisition."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
from build import ROOT, SPEC_PATH, artifact_name, digest
from binary import machine


def require(ok, detail):
    if not ok:
        raise ValueError(detail)


def archive_files(path):
    files = {}
    with tarfile.open(path) as archive:
        for member in archive:
            name=member.name
            require(not name.startswith('/') and '\\' not in name and ':' not in name and
                    '..' not in PurePosixPath(name).parts, 'Unsafe archive path')
            if member.isdir():
                continue
            require(member.isfile() and name not in files, 'Archive links, special files and duplicate paths are prohibited')
            files[name]=archive.extractfile(member).read()
    return files


def validate_payload(files, spec, target, core_revision=None, clean=False):
    def obj(name):
        require(name in files,'Missing runtime evidence: '+name)
        return json.loads(files[name])
    require(obj('spec.json')==spec,'Embedded runtime specification mismatch')
    b=obj('build.json');v=obj('validation.json');r=obj('repeat-build.json');p=obj('source-provenance.json')
    spec_hash=hashlib.sha256(SPEC_PATH.read_bytes()).hexdigest()
    require(b.get('target')==target and b.get('recipe')==spec['recipe'] and
            b.get('version')==spec['source']['version'] and b.get('source_revision')==spec['source']['revision'] and
            b.get('spec_sha256')==spec_hash,'Build identity mismatch')
    if target == 'windows-x86_64':
        regression=b.get('compiler_regression',{})
        require(regression.get('status')=='passed' and regression.get('widths')==list(range(88,97)) and
                regression.get('source_sha256')==digest(ROOT/'tests/fixtures/compiler/lrintf-alignment.c') and
                '-fno-builtin-lrintf' in b.get('build_options',{}).get('c_flags','').split(),
                'Missing Windows compiler regression qualification')
    if core_revision:
        require(b.get('core_revision')==core_revision,'Unexpected Core build revision')
    if clean:
        require(b.get('core_worktree_modified') is False,'Release build was not clean')
    scripts={x.name:digest(x) for x in (ROOT/'scripts/ffmpeg').glob('*.py')}
    require(b.get('build_scripts_sha256')==scripts,'Build recipe scripts mismatch')
    require(v.get('baseline') is False and v.get('target')==target and bool(v.get('smoke')) and
            bool(v.get('linkage')),'Incomplete native runtime validation')
    require(v.get('smoke',{}).get('gblur-row-alignment')=='passed', 'Missing gblur alignment regression')
    host=b.get('native_host',{})
    expected_os='Windows' if target.startswith('windows-') else ('Darwin' if target.startswith('macos-') else 'Linux')
    native_arch=host.get('machine','').lower()
    native_arch={'amd64':'x86_64','aarch64':'arm64'}.get(native_arch,native_arch)
    require(host.get('system')==expected_os and target.endswith('-'+native_arch), 'Native qualification host mismatch')
    if target.startswith('windows-'):
        notices=b.get('compiler_runtime_notices',{})
        require(bool(notices) and all('licenses/'+n in files and hashlib.sha256(files['licenses/'+n]).hexdigest()==h for n,h in notices.items()),
                'Missing compiler runtime license notices')
    suffix='.exe' if target.startswith('windows-') else ''
    pair={name+suffix for name in ['ffmpeg','ffprobe']}
    require(set(v.get('binary_sha256',{}))==pair,'Exactly one validated FFmpeg/FFprobe pair required')
    require({name for name in files if PurePosixPath(name).name in {'ffmpeg','ffprobe','ffmpeg.exe','ffprobe.exe'}}==pair,
            'Unexpected or duplicate runtime executable')
    hashes={name:hashlib.sha256(files[name]).hexdigest() for name in pair if name in files}
    require(hashes==v['binary_sha256'],'Runtime binary changed after validation')
    for name in pair:
        machine(files[name],target)
    require(r.get('status')=='passed' and r.get('target')==target and r.get('spec_sha256')==spec_hash and
            r.get('core_revision')==b.get('core_revision') and
            r.get('binary_sha256')=={n:{'first':h,'second':h} for n,h in hashes.items()},'Missing or mismatched repeat-build evidence')
    require(p.get('status')=='passed' and p.get('archive_sha256')==spec['source']['sha256'] and
            p.get('source_revision')==spec['source']['revision'] and p.get('source_date_epoch')==spec['source_date_epoch'] and
            p.get('release_key_fingerprint')==spec['source']['release_key_fingerprint'] and
            p.get('tag_key_fingerprint')==spec['source']['tag_key_fingerprint'] and p.get('compared_files',0)>0,
            'Missing or mismatched official provenance')
    t=next(t for t in spec['targets'] if t['id']==target)
    for category,required in spec['required'].items():
        available=b.get('parsers',[]) if category=='parsers' else v.get('capabilities',{}).get(category,[])
        expected=required+(t['required_encoders'] if category=='encoders' else [])
        require(set(expected)<=set(available),'Missing validated '+category)
    require('core-tests-passed.txt' in files,'Missing Core integration evidence')
    for name in t['dependencies']+['ffmpeg']:
        require(any(k.startswith('licenses/'+name+'/') for k in files),'Missing actual license notices: '+name)
    source=obj('SOURCE.json')
    require(source.get('asset')==artifact_name(spec,target)+'-sources.tar.gz' and
            source.get('repository')==spec['release_repository'] and
            source.get('tag')==f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}' and
            re.fullmatch('[0-9a-f]{64}',source.get('sha256','')),'Invalid corresponding-source mapping')
    return b


def validate_runtime(path, spec, target, core_revision=None, clean=False):
    files=archive_files(path)
    prefix=artifact_name(spec,target)+'/'
    require(all(k.startswith(prefix) for k in files),'Unexpected runtime archive root')
    files={k[len(prefix):]:v for k,v in files.items()}
    require('SHA256SUMS' in files,'Missing internal checksums')
    expected={}
    for line in files['SHA256SUMS'].decode().splitlines():
        match=re.fullmatch(r'([0-9a-f]{64})  (.+)',line)
        require(match is not None and match[2] not in expected,'Invalid or duplicate checksum entry')
        expected[match[2]]=match[1]
    actual={k:hashlib.sha256(v).hexdigest() for k,v in files.items() if k!='SHA256SUMS'}
    require(actual==expected,'Incomplete or mismatched internal checksums')
    validate_payload(files,spec,target,core_revision,clean)
    return files


def validate_sources(path,spec,target):
    files=archive_files(path)
    require(files.get('runtime/ffmpeg/spec.json')==SPEC_PATH.read_bytes(),'Source archive specification mismatch')
    for p in (ROOT/'scripts/ffmpeg').glob('*.py'):
        require(files.get('scripts/ffmpeg/'+p.name)==p.read_bytes(),'Corresponding build script mismatch')
    t=next(t for t in spec['targets'] if t['id']==target)
    for name in ['ffmpeg',*t['dependencies']]:
        item=spec['source'] if name=='ffmpeg' else spec['dependencies'][name]
        filename=item['url'].rsplit('/',1)[1] if 'url' in item else name+'-'+item['revision']+'.tar'
        data=files.get('sources/'+filename)
        require(data is not None,'Missing corresponding dependency source: '+name)
        if 'sha256' in item:
            require(hashlib.sha256(data).hexdigest()==item['sha256'],'Corresponding source checksum mismatch: '+name)
    return files
