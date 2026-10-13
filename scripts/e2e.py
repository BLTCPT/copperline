#!/usr/bin/env python3
"""Process-level synthetic workflow. No credentials or external business calls."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/copperline-cli')
args = parser.parse_args()
binary = str(Path(args.binary).resolve())

with tempfile.TemporaryDirectory(prefix='copperline-e2e-') as directory:
    def start():
        return subprocess.Popen([binary, '--data', directory, 'api'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    process = start()
    revision = 0
    request_count = 0
    def call(method, params=None):
        global request_count
        request_count += 1
        process.stdin.write(json.dumps({'method': method, 'params': params}) + '\n')
        process.stdin.flush()
        line = process.stdout.readline()
        if not line:
            raise RuntimeError('API exited: ' + process.stderr.read())
        return json.loads(line)
    def execute(command, params=None, name='Pricing', role='analyst', fail=False, request_id=None):
        global revision
        envelope = {'request_id': request_id or f'e2e-{request_count}', 'expected_revision': revision,
                    'actor': {'name': name, 'role': role}, 'command': command}
        if params is not None:
            envelope['params'] = params
        result = call('execute', envelope)
        if fail:
            assert not result['ok'], result
            assert result['revision'] == revision, result
        else:
            assert result['ok'], result
            revision = result['result']['revision']
        return envelope, result
    execute('seed_demo', name='Administrator', role='admin')
    price = {'product_id': 'VLV-100', 'customer_id': 'C-ATLAS', 'date': '2026-10-13',
             'quantity': 10000, 'discount': 1000, 'rebate': 500}
    evaluation = call('evaluate', price)
    assert evaluation['ok'], evaluation
    golden = evaluation['result']
    assert [golden[k] for k in ['invoice_total', 'rebate_total', 'pocket_total', 'margin_total']] == [90000, 4500, 85500, 25500], golden
    assert golden['margin_bps'] == 2982
    execute('quote_create', {'id': 'Q-E2E', 'title': 'Synthetic service program', 'customer_id': 'C-ATLAS', 'date': '2026-10-13'}, 'Sales', 'sales')
    envelope, outcome = execute('quote_add_line', {'id': 'Q-E2E', 'version': 1, 'request': price}, 'Sales', 'sales')
    replay = call('execute', envelope)
    assert replay['ok'] and replay['result']['replayed'] and replay['result']['revision'] == revision
    execute('quote_submit', {'id': 'Q-E2E', 'version': 1}, 'Sales', 'sales', fail=True)
    execute('quote_submit', {'id': 'Q-E2E', 'version': 2}, 'Sales', 'sales')
    execute('quote_decide', {'id': 'Q-E2E', 'version': 3, 'approve': True, 'note': 'Self approval must fail'}, 'Sales', 'admin', fail=True)
    execute('quote_decide', {'id': 'Q-E2E', 'version': 3, 'approve': True, 'note': 'Reviewed in synthetic E2E'}, 'Approver', 'approver')
    exported = call('export_quote', {'id': 'Q-E2E'})
    assert exported['ok'] and '855.00' in exported['result']['csv']
    execute('quote_add_line', {'id': 'Q-E2E', 'version': 4, 'request': price}, 'Sales', 'sales', fail=True)
    execute('products_import', {'csv': 'id,name,family,unit,currency,list_price\nNEW,New valve,Flow,each,USD,50.00\nVLV-100,Duplicate,Flow,each,USD,5.00'}, fail=True)
    assert 'NEW' not in call('state')['result']['products']
    execute('list_calculate', {'id': 'PL-E2E', 'name': 'Synthetic October list', 'customer_id': 'C-ATLAS', 'date': '2026-10-13', 'quantity': 1000, 'currency': 'USD'})
    execute('list_publish', {'id': 'PL-E2E', 'version': 1}, fail=True)
    execute('list_submit', {'id': 'PL-E2E', 'version': 1})
    execute('list_decide', {'id': 'PL-E2E', 'version': 2, 'approve': True, 'note': 'Whole-list commercial review'}, 'Approver', 'approver')
    execute('list_publish', {'id': 'PL-E2E', 'version': 3})
    state = call('state')['result']
    assert state['price_lists']['PL-E2E']['status'] == 'published'
    assert state['quotes']['Q-E2E']['status'] == 'approved'
    assert len(state['audit']) == revision
    process.stdin.write('{malformed\n')
    process.stdin.flush()
    assert not json.loads(process.stdout.readline())['ok']
    assert call('state')['result'] == state
    process.stdin.close()
    assert process.wait(timeout=10) == 0
    process = start()
    assert call('state')['result'] == state
    assert call('export_quote', {'id': 'Q-E2E'}) == exported
    execute('quote_revise', {'id': 'Q-E2E', 'version': 4}, 'Sales', 'sales')
    assert not call('export_quote', {'id': 'Q-E2E'})['ok']
    process.stdin.close()
    assert process.wait(timeout=10) == 0
    print(json.dumps({'result': 'PASS', 'api_requests': request_count + 1, 'committed_revisions': revision,
                      'golden_usd': {'invoice': '900.00', 'rebate': '45.00', 'pocket': '855.00', 'margin': '255.00'},
                      'verified': ['quote approval/export/revise', 'list approve/publish', 'idempotent retry', 'stale version',
                                   'separate approver', 'atomic import', 'malformed input survival', 'process restart persistence']}, indent=2))
