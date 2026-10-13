#!/usr/bin/env python3
"""Loopback concurrency, command idempotency and interrupted-write recovery. Synthetic data only."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/copperline-cli')
args = parser.parse_args()
binary = str(Path(args.binary).resolve())


def command(key, revision, customer):
    return {'request_id': key, 'expected_revision': revision,
            'actor': {'name': 'Synthetic transport reviewer', 'role': 'admin'},
            'command': 'customer_put',
            'params': {'id': customer, 'name': 'Synthetic transport customer', 'segment': 'OEM'}}


class Client:
    def __init__(self, port):
        self.socket = socket.create_connection(('127.0.0.1', port), timeout=2)
        self.stream = self.socket.makefile('rwb')

    def call(self, method, params=None):
        return self.raw(json.dumps({'method': method, 'params': params}).encode())

    def raw(self, line):
        self.stream.write(line + b'\n')
        self.stream.flush()
        response = self.stream.readline()
        assert response, 'Unexpected closed connection'
        return json.loads(response)

    def close(self):
        self.stream.close()
        self.socket.close()


with tempfile.TemporaryDirectory(prefix='copperline-transport-') as directory:
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    with tempfile.TemporaryFile(mode='w+t') as log:
        process = subprocess.Popen([binary, '--data', directory, 'serve', str(port)],
                                   stdout=subprocess.DEVNULL, stderr=log)
        clients = []
        try:
            deadline = time.monotonic() + 10
            while True:
                try:
                    a = Client(port)
                    clients.append(a)
                    break
                except OSError:
                    if process.poll() is not None or time.monotonic() > deadline:
                        log.seek(0)
                        raise RuntimeError('Server startup failed: ' + log.read())
                    time.sleep(0.05)
            assert a.call('catalog')['ok']
            b = Client(port)
            clients.append(b)
            # An existing client remains open; a second client must receive its own response.
            assert b.call('catalog')['ok']
            assert a.call('catalog')['ok']
            barrier = threading.Barrier(2)

            def competing(client, request):
                barrier.wait(timeout=5)
                return client.call('execute', request)

            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(competing, a, command('race-A', 0, 'C-A')),
                           pool.submit(competing, b, command('race-B', 0, 'C-B'))]
                raced = [f.result(timeout=5) for f in futures]
            assert sum(r['ok'] for r in raced) == 1, raced
            assert next(r for r in raced if not r['ok'])['revision'] == 1, raced
            assert 'Workspace changed' in next(r for r in raced if not r['ok'])['error'], raced
            identical = command('same-retry', 1, 'C-SAME')
            barrier = threading.Barrier(2)
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(competing, a, identical), pool.submit(competing, b, identical)]
                retried = [f.result(timeout=5) for f in futures]
            assert all(r['ok'] and r['result']['revision'] == 2 for r in retried), retried
            assert sorted(r['result']['replayed'] for r in retried) == [False, True], retried
            assert not a.call('execute', command('same-retry', 1, 'C-DIFFERENT'))['ok']
            assert not a.raw(b'{malformed')['ok']
            state = a.call('state')['result']
            assert state['revision'] == 2 and len(state['audit']) == 2
            # Recovery cannot inspect or mutate files while a business process owns the lock.
            locked = subprocess.run([binary, '--data', directory, 'storage-inspect'],
                                    capture_output=True, text=True, encoding="utf-8", timeout=10)
            assert locked.returncode != 0 and 'already open' in locked.stderr, locked
            oversized = Client(port)
            clients.append(oversized)
            try:
                oversized.socket.sendall(b'x' * (2 * 1024 * 1024 + 2))
                assert oversized.socket.recv(1024) == b''
            except (BrokenPipeError, ConnectionResetError):
                pass
            assert a.call('catalog')['ok']
            for client in clients:
                client.close()
            clients.clear()
            time.sleep(0.1)
            # Sixteen live, responsive connections consume all permits.
            for _ in range(16):
                client = Client(port)
                clients.append(client)
                assert client.call('catalog')['ok']
            extra = Client(port)
            try:
                busy = extra.call('catalog')
                assert not busy['ok'] and 'connection limit' in busy['error'], busy
            finally:
                extra.close()
            clients.pop().close()
            deadline = time.monotonic() + 2
            while True:
                replacement = Client(port)
                result = replacement.call('catalog')
                if result['ok']:
                    clients.append(replacement)
                    break
                replacement.close()
                assert time.monotonic() < deadline, 'Released permit was not returned'
                time.sleep(0.02)
            for client in clients:
                client.close()
            clients.clear()
            time.sleep(0.1)
            idle = Client(port)
            clients.append(idle)
            idle.socket.settimeout(8)
            assert idle.socket.recv(1024) == b'', 'Idle connection did not time out'
        finally:
            for client in clients:
                client.close()
            process.terminate()
            process.wait(timeout=10)
        reopened = subprocess.run([binary, '--data', directory, 'state'], check=True,
                                  capture_output=True, text=True, encoding="utf-8", timeout=10)
        assert json.loads(reopened.stdout) == state

with tempfile.TemporaryDirectory(prefix='copperline-recovery-process-') as directory:
    def cli(*parts, succeeds=True):
        p = subprocess.run([binary, '--data', directory, *parts],
                           capture_output=True, text=True, encoding="utf-8", timeout=10)
        assert (p.returncode == 0) == succeeds, p
        return json.loads(p.stdout) if succeeds else p.stderr

    initial = cli('state')
    pending_name = '00000000000000000001.pending'
    pending = Path(directory, pending_name)
    content = b'{"interrupted":"synthetic financial write'
    pending.write_bytes(content)
    digest = hashlib.sha256(content).hexdigest()
    report = cli('storage-inspect')
    assert report['committed_revision'] == 0 and report['next_revision'] == 1
    assert report['pending'][0]['eligible_for_quarantine']
    assert report['pending'][0]['sha256'] == digest
    request = command('retry-after-crash', 0, 'C-RECOVERED')
    cli('execute', json.dumps(request), succeeds=False)
    assert cli('state') == initial
    cli('quarantine-pending', pending_name, '0' * 64, succeeds=False)
    assert pending.read_bytes() == content
    recovered = cli('quarantine-pending', pending_name, digest)
    preserved = Path(recovered['preserved_path'])
    assert preserved.read_bytes() == content and not pending.exists()
    assert cli('state') == initial  # Evidence preservation is not a business commit.
    assert cli('execute', json.dumps(request))['revision'] == 1
    assert cli('execute', json.dumps(request))['replayed']
    assert cli('state')['customers']['C-RECOVERED']['name'] == 'Synthetic transport customer'
    assert preserved.read_bytes() == content

print(json.dumps({'result': 'PASS', 'connection_limit': 16, 'timeout_seconds': 5,
                  'verified': ['concurrent open clients', 'optimistic conflict serialization',
                               'simultaneous exact retry', 'changed-content retry rejection',
                               'malformed input survival', 'oversized client isolation',
                               'connection limit and permit return', 'idle timeout',
                               'restart persistence', 'exclusive recovery lock',
                               'pending inspection and checksum mismatch',
                               'evidence quarantine without promotion', 'retry after interrupted write']}, indent=2))
