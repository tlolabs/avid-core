"""Bind production qualification to immutable archives, independently of build inputs."""
import json
from artifact import require
from build import SPEC_PATH, artifact_name, digest

MANIFEST = 'manifest.json'


def verify_manifest(manifest, spec, revision):
    require(manifest.get('schema')==1 and manifest.get('status')=='qualified' and
            manifest.get('core_revision')==revision and manifest.get('spec_sha256')==digest(SPEC_PATH) and
            manifest.get('recipe')==spec['recipe'] and manifest.get('source')==spec['source'] and
            manifest.get('tag')==f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}',
            'Release manifest identity or qualification mismatch')
    policy=manifest.get('qualification_policy',{})
    require(policy.get('host_packaging') in {'required','downstream'} and bool(policy.get('scope')),
            'Missing explicit application packaging qualification scope')
    targets=manifest.get('targets',{})
    require(set(targets)=={t['id'] for t in spec['targets']},'Release manifest matrix incomplete')
    for t in spec['targets']:
        entry=targets[t['id']];name=artifact_name(spec,t['id'])
        require(entry.get('status')=='passed' and entry.get('qualification_os')==t['qualification_os'] and
                bool(entry.get('native_host')) and entry.get('runtime',{}).get('asset')==name+'.tar.gz' and
                entry.get('sources',{}).get('asset')==name+'-sources.tar.gz', 'Target qualification mapping mismatch')
        for kind in ['runtime','sources']:
            import re
            require(re.fullmatch('[0-9a-f]{64}',entry[kind].get('sha256','')),'Invalid manifest checksum')
    return manifest
