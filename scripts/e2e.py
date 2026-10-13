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
        return subprocess.Popen([binary, '--data', directory, 'api'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
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
    execute('grid_calculate', {'id': 'G-E2E', 'version': 0, 'request': price})
    execute('grid_submit', {'id': 'G-E2E', 'version': 1})
    execute('grid_decide', {'id': 'G-E2E', 'version': 2, 'approve': True, 'note': 'Activate item'}, 'Approver', 'approver')
    changed = dict(price, discount=1500)
    execute('grid_calculate', {'id': 'G-E2E', 'version': 3, 'request': changed})
    grid = call('state')['result']['grid']['G-E2E']
    assert grid['candidate']['invoice_unit'] == 8500 and grid['last_approved']['evaluation']['invoice_unit'] == 9000
    execute('grid_submit', {'id': 'G-E2E', 'version': 4})
    execute('grid_decide', {'id': 'G-E2E', 'version': 5, 'approve': False, 'note': 'Preserve approved price'}, 'Approver', 'approver')
    terms = {'id': 'A-E2E', 'name': 'Synthetic annual rebate', 'customer_id': 'C-ATLAS', 'currency': 'USD',
             'start': '2026-01-01', 'end': '2027-01-01', 'tiers': [{'threshold': 0, 'rate': 200}, {'threshold': 100000, 'rate': 500}]}
    execute('agreement_create', {'terms': terms})
    execute('agreement_submit', {'id': 'A-E2E', 'version': 1})
    execute('agreement_decide', {'id': 'A-E2E', 'version': 2, 'approve': True, 'note': 'Review retrospective tiers'}, 'Approver', 'approver')
    header = 'id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\n'
    row1 = 'T-1,synthetic-e2e,VLV-100,C-ATLAS,2026-10-13,10,each,USD,900.00,600.00\n'
    row2 = 'T-2,synthetic-e2e,VLV-100,C-ATLAS,2026-10-14,1,each,USD,100.00,50.00\n'
    execute('transactions_import', {'csv': header + row1 + row2})
    execute('transactions_import', {'csv': header + row1}, fail=True)
    execute('rebate_accrue', {'id': 'R-E2E', 'agreement_id': 'A-E2E', 'version': 3})
    accrual = call('state')['result']['accruals']['R-E2E']
    assert accrual['eligible_invoice'] == 100000 and accrual['rebate'] == 5000
    actuals = call('analytics')
    assert actuals['ok'] and actuals['result'][0]['invoice_margin'] == 35000 and actuals['result'][0]['margin_bps'] == 3500, actuals
    # M3: explicit approved source, owner-only authorship and frozen two-stage review.
    source_request = dict(price, discount=500, rebate=0, source={'kind': 'grid', 'id': 'G-E2E', 'approval_revision': grid['last_approved']['decision']['revision']})
    thresholds = {'max_discount': 1000, 'min_margin': 2000, 'usd_value': 80000, 'eur_value': 80000, 'gbp_value': 80000}
    execute('quote_approval_policy_set', {'expected_version': 0, 'thresholds': thresholds}, 'Administrator', 'admin')
    execute('quote_create', {'id': 'Q-M3', 'title': 'Approved grid with sequential review', 'customer_id': 'C-ATLAS', 'date': '2026-10-13'}, 'Sales', 'sales')
    execute('quote_add_line', {'id': 'Q-M3', 'version': 1, 'request': source_request}, 'Sales', 'sales')
    execute('quote_reprice', {'id': 'Q-M3', 'version': 2}, 'Other administrator', 'admin', fail=True)
    execute('quote_submit', {'id': 'Q-M3', 'version': 2}, 'Sales', 'sales')
    execute('quote_decide', {'id': 'Q-M3', 'version': 3, 'approve': True, 'note': 'Cannot skip commercial review'}, 'Finance', 'finance', fail=True)
    first_review, _ = execute('quote_decide', {'id': 'Q-M3', 'version': 3, 'approve': True, 'note': 'Commercial review'}, 'Approver', 'approver')
    assert not call('export_quote', {'id': 'Q-M3'})['ok']
    execute('quote_decide', {'id': 'Q-M3', 'version': 4, 'approve': True, 'note': 'Admin cannot substitute Finance'}, 'Administrator', 'admin', fail=True)
    relaxed = dict(thresholds, max_discount=10000, min_margin=0, usd_value=10000000)
    execute('quote_approval_policy_set', {'expected_version': 1, 'thresholds': relaxed}, 'Administrator', 'admin')
    pending = call('state')['result']['quotes']['Q-M3']
    assert pending['status'] == 'pending' and len(pending['governance']['cycles'][-1]['stages']) == 2
    assert pending['governance']['cycles'][-1]['policy']['version'] == 1
    execute('quote_decide', {'id': 'Q-M3', 'version': 4, 'approve': True, 'note': 'Same person cannot review twice'}, 'Approver', 'finance', fail=True)
    execute('quote_decide', {'id': 'Q-M3', 'version': 4, 'approve': True, 'note': 'Independent Finance review'}, 'Finance', 'finance')
    repeat_review = call('execute', first_review)
    assert repeat_review['ok'] and repeat_review['result']['replayed']
    m3_export = call('export_quote', {'id': 'Q-M3'})
    assert m3_export['ok'] and 'G-E2E' in m3_export['result']['csv']
    execute('grid_calculate', {'id': 'G-E2E', 'version': 6, 'request': changed})
    execute('grid_submit', {'id': 'G-E2E', 'version': 7})
    execute('grid_decide', {'id': 'G-E2E', 'version': 8, 'approve': True, 'note': 'New active source'}, 'Approver', 'approver')
    assert not call('evaluate', source_request)['ok']
    assert call('export_quote', {'id': 'Q-M3'}) == m3_export
    from_list = dict(price, quantity=1000, discount=0, rebate=0, source={'kind': 'price_list', 'id': 'PL-E2E', 'version': 4})
    assert call('evaluate', from_list)['result']['source']['source']['id'] == 'PL-E2E'
    execute('rebate_accrue', {'id': 'R-E2E-SECOND-SNAPSHOT', 'agreement_id': 'A-E2E', 'version': 3})
    repeated_accrual = call('state')['result']['accruals']['R-E2E-SECOND-SNAPSHOT']
    assert repeated_accrual['rebate'] == accrual['rebate']
    assert call('analytics') == actuals  # Alternative snapshots never add liabilities to actuals.
    state = call('state')['result']
    assert state['quotes']['Q-M3']['status'] == 'approved'
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
    assert call('export_quote', {'id': 'Q-M3'}) == m3_export
    execute('quote_revise', {'id': 'Q-E2E', 'version': 4}, 'Sales', 'sales')
    assert not call('export_quote', {'id': 'Q-E2E'})['ok']
    process.stdin.close()
    assert process.wait(timeout=10) == 0
    print(json.dumps({'result': 'PASS', 'api_requests': request_count + 1, 'committed_revisions': revision,
                      'golden_usd': {'invoice': '900.00', 'rebate': '45.00', 'pocket': '855.00', 'margin': '255.00'},
                      'verified': ['quote approval/export/revise', 'list approve/publish', 'idempotent retry', 'stale version',
                                   'separate approver', 'atomic import', 'malformed input survival', 'process restart persistence', 'grid candidate/active separation', 'tier agreement approval', 'atomic transaction import', 'frozen accrual', 'invoice analytics', 'owner-only quote editing', 'conditional sequential Finance review', 'frozen route policy', 'decision retry after completion', 'approved grid/list pricing', 'source replacement preserves quote export', 'repeated accrual snapshots do not change actuals']}, indent=2))
