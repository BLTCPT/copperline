#!/usr/bin/env python3
"""Rebuild exact dependency notices offline from locked archives and pinned supplements.

Requires Python 3.11+. Run cargo fetch --locked beforehand when the registry cache
is empty. Never downloads or executes dependency code. --check is read-only;
--require-complete also rejects explicitly documented upstream notice gaps.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MAX_ARCHIVE = 64 * 1024 * 1024
MAX_NOTICE = 2 * 1024 * 1024
MAX_OUTPUT = 32 * 1024 * 1024
NOTICE_NAME = re.compile(r"^(licen[cs]e|copying|copyright|notice|authors)(?:$|[._-])", re.I)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_bounded(path, limit):
    with path.open('rb') as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f'Input exceeds size limit: {path.name}')
    return data


def safe_relative(value):
    path = PurePosixPath(value)
    if '\\' in value or ':' in value or path.is_absolute() or not path.parts or any(p in ('', '.', '..') for p in path.parts):
        raise ValueError('Unsafe notice path')
    return path


def notice_name(path, package):
    name = path.name
    if package == 'epaint_default_fonts' and path.parts[0] == 'fonts' and path.suffix == '.txt':
        return True
    return bool(NOTICE_NAME.match(name)) and path.suffix.lower() not in ('.rs', '.c', '.h')


def archive_notices(package, locked):
    base = Path(package['manifest_path']).parent
    archive = base.parent.parent.parent / 'cache' / base.parent.name / (base.name + '.crate')
    data = read_bounded(archive, MAX_ARCHIVE)
    if sha256(data) != locked['checksum']:
        raise ValueError(f'Cargo.lock checksum mismatch: {package["name"]} {package["version"]}')
    expected_prefix = f'{package["name"]}-{package["version"]}'
    result = []
    vcs = None
    metadata = None
    seen = set()
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as crate:
        for member in crate:
            full = safe_relative(member.name)
            if full.parts[0] != expected_prefix:
                raise ValueError('Unexpected crate archive root')
            if len(full.parts) == 1:
                continue
            relative = PurePosixPath(*full.parts[1:])
            selected = notice_name(relative, package['name'])
            if not selected and str(relative) not in ('.cargo_vcs_info.json', 'Cargo.toml.orig'):
                continue
            if not member.isfile() or member.size > MAX_NOTICE or str(relative) in seen:
                raise ValueError('Invalid or duplicate archive notice')
            seen.add(str(relative))
            stream = crate.extractfile(member)
            if stream is None:
                raise ValueError('Unreadable archive notice')
            raw = stream.read(MAX_NOTICE + 1)
            if len(raw) != member.size:
                raise ValueError('Truncated archive notice')
            raw.decode('utf-8')
            if selected:
                result.append((str(relative), raw))
            elif str(relative) == '.cargo_vcs_info.json':
                vcs = json.loads(raw)
            else:
                metadata = raw
    return sorted(result), vcs, metadata


def load_supplements(locked):
    manifest = json.loads((ROOT / 'legal/upstream-sources.json').read_text(encoding='utf-8'))
    if manifest['format_version'] != 1:
        raise ValueError('Unsupported upstream notice manifest')
    result = {}
    for source in manifest['sources']:
        revision = source['revision']
        if not re.fullmatch(r'[a-f0-9]{40}', revision):
            raise ValueError('Upstream revision must be an exact commit')
        repository = source['repository']
        if not re.fullmatch(r'https://github.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
            raise ValueError('Supplement must identify an official GitHub repository')
        prefix = repository.replace('https://github.com/', 'https://raw.githubusercontent.com/') + '/' + revision + '/'
        files = []
        for file in source['files']:
            relative = safe_relative(file['path'])
            path = ROOT / 'legal' / relative
            if not file['source_url'].startswith(prefix):
                raise ValueError('Supplement URL is not pinned to its recorded revision')
            raw = read_bounded(path, MAX_NOTICE)
            if sha256(raw) != file['sha256']:
                raise ValueError(f'Upstream notice checksum mismatch: {relative}')
            raw.decode('utf-8')
            files.append((str(relative), file['source_url'], raw))
        if not files and not source.get('unresolved'):
            raise ValueError('Missing upstream notices without a documented gap')
        for package in source['packages']:
            key = package['name'], package['version']
            if key in result or key not in locked or package['checksum'] != locked[key]['checksum']:
                raise ValueError('Duplicate, stale or unlocked upstream notice mapping')
            result[key] = (source, package, sorted(files))
    return result


def standard_licenses():
    manifest = json.loads((ROOT / 'legal/standard-sources.json').read_text(encoding='utf-8'))
    if manifest['format_version'] != 1:
        raise ValueError('Unsupported standard license manifest')
    result = {}
    for item in manifest['licenses']:
        key = item['spdx']
        relative = safe_relative(item['path'])
        raw = read_bounded(ROOT / 'legal' / relative, MAX_NOTICE)
        if key in result or sha256(raw) != item['sha256']:
            raise ValueError('Duplicate or changed standard license text')
        raw.decode('utf-8')
        result[key] = (item, raw)
    return result


def generate():
    lock_data = tomllib.loads((ROOT / 'Cargo.lock').read_text(encoding='utf-8'))
    locked = {(p['name'], p['version']): p for p in lock_data['package'] if 'source' in p}
    if any(p['source'] != 'registry+https://github.com/rust-lang/crates.io-index' for p in locked.values()):
        raise ValueError('Non-crates.io dependencies need explicit notice handling')
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'], cwd=ROOT, text=True))
    packages = [p for p in metadata['packages'] if p['source'] is not None]
    if {(p['name'], p['version']) for p in packages} != set(locked):
        raise ValueError('Cargo metadata does not cover every external Cargo.lock package')
    supplements = load_supplements(locked)
    standards = standard_licenses()
    output = [b'Copperline third-party source license notices\n'
              b'All external Cargo.lock packages, including optional and cross-platform dependencies.\n'
              b'Exact text from checksum-verified crate archives and pinned official upstream supplements.\n'
              b'See legal/notice-coverage.json and legal/upstream-sources.json for provenance and gaps.\n']
    coverage = []
    for package in sorted(packages, key=lambda p: (p['name'], p['version'])):
        key = package['name'], package['version']
        files, vcs, cargo_original = archive_notices(package, locked[key])
        output.append(('\n' + '=' * 72 + '\n' + ' '.join(key) + ' | ' + str(package['license']) + '\n'
                       + str(package['repository']) + '\nCargo.lock archive SHA256: ' + locked[key]['checksum'] + '\n').encode())
        row = {'name': key[0], 'version': key[1], 'license': package['license'],
               'archive_sha256': locked[key]['checksum'], 'archive_notices': [], 'upstream_notices': []}
        for filename, raw in files:
            output.extend([('\nArchive file: ' + filename + '\n').encode(), raw, b'\n'])
            row['archive_notices'].append({'path': filename, 'sha256': sha256(raw)})
        if key in supplements:
            source, pinned_package, upstream = supplements[key]
            if vcs is None or vcs['git']['sha1'] != source['revision'] or vcs.get('path_in_vcs', '') != pinned_package['path_in_vcs']:
                raise ValueError(f'Published crate VCS provenance mismatch: {key}')
            for filename, url, raw in upstream:
                output.extend([('\nOfficial upstream: ' + url + '\n').encode(), raw, b'\n'])
                row['upstream_notices'].append({'path': filename, 'sha256': sha256(raw), 'source_url': url})
            if source.get('standard_license'):
                standard = source['standard_license']
                if standard != 'MIT' or not re.search(r'\bMIT\b', package['license']):
                    raise ValueError('Standard notice option is not declared by this package')
                item, raw = standards[standard]
                output.extend([('\nStandard ' + standard + ' license option: ' + item['source_url'] + '\n'
                               + item['note'] + '\n').encode(), raw, b'\n'])
                row['standard_license'] = item
            if source.get('upstream_legal_caveat'):
                row['upstream_legal_caveat'] = source['upstream_legal_caveat']
                output.append(('\nUPSTREAM LEGAL CAVEAT: ' + source['upstream_legal_caveat'] + '\n').encode())
            if source.get('unresolved'):
                row['unresolved'] = source['unresolved']
                if cargo_original is None:
                    raise ValueError('Missing original metadata for documented upstream gap')
                output.extend([('\nUPSTREAM NOTICE GAP: ' + source['unresolved'] + '\n'
                               'Exact published Cargo.toml.orig metadata follows; this is not a substitute license text.\n').encode(),
                               cargo_original, b'\n'])
        if not files and key not in supplements:
            raise ValueError(f'No license text or reviewed upstream gap: {key}')
        coverage.append(row)
    combined = b''.join(output)
    if len(combined) > MAX_OUTPUT:
        raise ValueError('Combined notice size exceeds limit')
    report = {'format_version': 1, 'scope': 'All external Cargo.lock packages; not a target-specific linked-binary inventory.',
              'cargo_lock_sha256': sha256((ROOT / 'Cargo.lock').read_bytes()),
              'package_count': len(packages), 'documented_upstream_gaps': sum('unresolved' in p for p in coverage),
              'packages_with_upstream_legal_caveats': sum('upstream_legal_caveat' in p for p in coverage),
              'packages': coverage}
    return combined, (json.dumps(report, indent=2) + '\n').encode(), report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Compare generated bytes without modifying files')
    parser.add_argument('--require-complete', action='store_true', help='Fail on documented missing upstream notice text')
    args = parser.parse_args()
    notices, coverage, report = generate()
    if args.require_complete and report['documented_upstream_gaps']:
        raise ValueError('Documented upstream notice gaps remain; inspect legal/notice-coverage.json')
    for name, content in [('THIRD_PARTY_NOTICES.txt', notices), ('notice-coverage.json', coverage)]:
        path = ROOT / 'legal' / name
        if args.check:
            if not path.exists() or path.read_bytes() != content:
                raise ValueError(f'Generated notice evidence differs: legal/{name}')
        else:
            path.write_bytes(content)
    print(f'{"Verified" if args.check else "Wrote"} notices for {report["package_count"]} locked dependencies; '
          f'{report["documented_upstream_gaps"]} documented upstream notice gap(s)')


if __name__ == '__main__':
    main()
