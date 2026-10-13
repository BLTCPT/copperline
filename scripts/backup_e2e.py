#!/usr/bin/env python3
"""Portable archive, original byte/export/retry evidence, and safe-new-target process flow."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/copperline-cli')
args = parser.parse_args()
binary = str(Path(args.binary).resolve())
checks = []
calls = 0


def cli(*arguments, okay=True):
    global calls
    calls += 1
    result = subprocess.run([binary, *map(str, arguments)], capture_output=True,
                            text=True, encoding='utf-8', timeout=60)
    assert (result.returncode == 0) == okay, (arguments, result.stdout, result.stderr)
    return json.loads(result.stdout) if okay else result.stderr


def inventory(root):
    return {str(p.relative_to(root)).replace('\\', '/'):
            ('directory' if p.is_dir() else hashlib.sha256(p.read_bytes()).hexdigest())
            for p in root.rglob('*') if p.name != 'workspace.lock'}


class Api:
    def __init__(self, root):
        self.process = subprocess.Popen([binary, '--data', str(root), 'api'],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, encoding='utf-8')

    def call(self, method, params=None, okay=True):
        global calls
        calls += 1
        self.process.stdin.write(json.dumps({'method': method, 'params': params}) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, self.process.stderr.read()
        result = json.loads(line)
        assert result['ok'] == okay, result
        return result.get('result') if okay else result

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=10) == 0
        self.process.stdout.close()
        self.process.stderr.close()

    def abort(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=10)


with tempfile.TemporaryDirectory(prefix='copperline-m7-backup-') as directory:
    root = Path(directory)
    source = root / 'source'
    archive = root / 'complete.clbak'
    destination = root / 'restored'
    api = Api(source)
    revision = 0

    def execute(command, params=None, role='analyst', name='Morgan'):
        global revision
        request = {'request_id': f'backup-{revision + 1}', 'expected_revision': revision,
                   'actor': {'name': name, 'role': role}, 'command': command}
        if params is not None:
            request['params'] = params
        result = api.call('execute', request)
        revision = result['revision']
        return request

    try:
        first = execute('seed_demo', role='admin', name='Casey')
        execute('conversion_import', {'id': 'BOOK', 'name': 'Synthetic backup rate evidence',
                                     'csv': Path('fixtures/conversions.csv').read_text(encoding='utf-8')})
        execute('conversion_submit', {'id': 'BOOK', 'version': 1})
        execute('conversion_decide', {'id': 'BOOK', 'version': 2, 'approve': True,
                                     'note': 'Independent rate review'}, role='approver', name='Riley')
        execute('quote_create', {'id': 'Q', 'title': 'Synthetic archived EUR box quote',
                                'customer_id': 'C-ATLAS', 'date': '2026-10-13'}, role='sales', name='Alex')
        execute('quote_add_line', {'id': 'Q', 'version': 1, 'request': {
            'product_id': 'VLV-100', 'customer_id': 'C-ATLAS', 'date': '2026-10-13',
            'quantity': 1000, 'discount': 0, 'rebate': 0,
            'conversion': {'currency': 'EUR', 'unit': 'box'}}}, role='sales', name='Alex')
        execute('quote_submit', {'id': 'Q', 'version': 2}, role='sales', name='Alex')
        execute('quote_decide', {'id': 'Q', 'version': 3, 'approve': True,
                                'note': 'Independent quote evidence review'}, role='approver', name='Riley')
        state = api.call('state')
        quote_csv = api.call('export_quote', {'id': 'Q'})['csv']
        assert state['quotes']['Q']['status'] == 'approved'
        assert 'conversion_rounding' in quote_csv
        cli('--data', source, 'backup', archive, okay=False)
        assert not archive.exists()
        checks.append('source lock prevents an inconsistent competing backup')
        api.close()

        next_name = f'{revision + 1:020}.pending'
        pending = source / next_name
        pending.write_bytes(b'{"interrupted synthetic write":')
        recovery = source / 'recovery' / 'prior-evidence'
        recovery.mkdir(parents=True)
        (recovery / 'bytes.pending').write_bytes(b'\x00synthetic recovery\xff\n')
        (recovery / 'empty').mkdir()
        original = inventory(source)
        summary = cli('--data', source, 'backup', archive)
        assert summary['committed_revision'] == revision and summary['snapshot_count'] == revision + 1
        assert summary['pending_count'] == 1 and summary['recovery_file_count'] == 1
        raw = archive.read_bytes()
        assert summary['sha256'] == hashlib.sha256(raw).hexdigest()
        assert summary['total_bytes'] == len(raw)
        inspected = cli('backup-inspect', archive)
        for field in ('sha256', 'committed_revision', 'snapshot_count', 'pending_count', 'recovery_file_count'):
            assert inspected[field] == summary[field]
        second = root / 'same-bytes.clbak'
        cli('--data', source, 'backup', second)
        assert second.read_bytes() == raw and inventory(source) == original
        checks.append('deterministic exact-byte archive preserves snapshots, pending and nested recovery evidence')

        cli('--data', source, 'backup', archive, okay=False)
        assert archive.read_bytes() == raw
        missing = root / 'does-not-exist'
        cli('--data', missing, 'backup', root / 'missing.clbak', okay=False)
        assert not missing.exists()
        foreign = source / 'unrecognized.csv'
        foreign.write_text('synthetic,do not omit\n', encoding='utf-8')
        cli('--data', source, 'backup', root / 'omission.clbak', okay=False)
        assert foreign.read_text(encoding='utf-8') == 'synthetic,do not omit\n'
        foreign.unlink()  # Only this test's owned fixture; business/recovery bytes remain unchanged.
        checks.append('existing archives, missing sources and unsupported source evidence fail without replacement or bootstrap')

        for label, damaged in [('truncated', raw[:-7]), ('trailing', raw + b'EXTRA'),
                               ('corrupt', raw[:len(raw)//2] + bytes([raw[len(raw)//2] ^ 1]) + raw[len(raw)//2+1:])]:
            bad_archive = root / f'{label}.clbak'
            bad_archive.write_bytes(damaged)
            new_target = root / f'{label}-restore'
            cli('backup-inspect', bad_archive, okay=False)
            cli('restore', bad_archive, new_target, okay=False)
            assert not new_target.exists() and bad_archive.read_bytes() == damaged
        assert inventory(source) == original
        checks.append('corrupt, truncated and trailing archive bytes reject before destination creation')

        occupied = root / 'existing-empty'
        occupied.mkdir()
        cli('restore', archive, occupied, okay=False)
        assert occupied.is_dir() and list(occupied.iterdir()) == []
        occupied_file = root / 'existing-file'
        occupied_file.write_bytes(b'preserve this exact owned test file')
        cli('restore', archive, occupied_file, okay=False)
        assert occupied_file.read_bytes() == b'preserve this exact owned test file'
        checks.append('restore refuses existing empty directories and files')

        restored = cli('restore', archive, destination)
        assert Path(restored['workspace']).resolve() == destination.resolve()
        assert inventory(destination) == original and inventory(source) == original
        api = Api(destination)
        assert api.call('state') == state
        assert api.call('export_quote', {'id': 'Q'})['csv'] == quote_csv
        assert api.call('execute', first)['replayed']
        mutation = {'request_id': 'after-restore', 'expected_revision': revision,
                    'actor': {'name': 'Casey', 'role': 'admin'}, 'command': 'customer_put',
                    'params': {'id': 'C-AFTER', 'name': 'Synthetic restored customer', 'segment': 'OEM'}}
        api.call('execute', mutation, okay=False)
        assert api.call('state') == state
        api.close()
        checks.append('restored approvals, conversion exports and original receipts reopen unchanged; pending still blocks next write')

        pending_report = cli('--data', destination, 'storage-inspect')
        pending_hash = hashlib.sha256(pending.read_bytes()).hexdigest()
        assert pending_report['next_revision'] == revision + 1
        preserved = cli('--data', destination, 'quarantine-pending', next_name, pending_hash)
        assert Path(preserved['preserved_path']).read_bytes() == pending.read_bytes()
        api = Api(destination)
        result = api.call('execute', mutation)
        assert result['revision'] == revision + 1 and not result['replayed']
        assert api.call('execute', mutation)['replayed']
        after = api.call('state')
        api.close()
        latest_archive = root / 'after-retry.clbak'
        cli('--data', destination, 'backup', latest_archive)
        final_target = root / 'restored-again'
        cli('restore', latest_archive, final_target)
        assert cli('--data', final_target, 'state') == after
        assert inventory(source) == original and archive.read_bytes() == raw
        checks.append('preserved pending quarantine then exact retry commits once and survives another complete backup/restore')
        print(json.dumps({'result': 'PASS', 'process_and_api_calls': calls,
                          'archived_revision': revision, 'archive_bytes': len(raw),
                          'archive_sha256': summary['sha256'], 'verified': checks}, indent=2))
    finally:
        api.abort()
