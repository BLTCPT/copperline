#!/usr/bin/env python3
"""Quantity credits, original-rate reporting and nominal close across real processes."""
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
from workload import Api

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/copperline-cli')
args = parser.parse_args()
binary = Path(args.binary).resolve(strict=True)
calls = revision = serial = 0

with tempfile.TemporaryDirectory(prefix='copperline-m9-flow-') as temporary:
    root = Path(temporary)
    data = root / 'workspace'
    evidence = root / 'evidence'
    evidence.mkdir()

    def start():
        global serial
        serial += 1
        return Api(binary, data, evidence, serial, 30)

    api = start()

    def call(method, params=None, okay=True):
        global calls
        calls += 1
        return api.call(method, params, okay)[0]

    def history():
        return {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in data.glob('*.json')}

    def execute(command, params=None, role='analyst', name='Morgan', okay=True):
        global revision
        request = dict(request_id=f'm9-{calls+1}', expected_revision=revision,
                       actor=dict(name=name, role=role), command=command)
        if params is not None:
            request['params'] = params
        before = (call('state'), history()) if not okay else None
        outcome = call('execute', request, okay)
        if okay:
            revision = outcome['revision']
        else:
            assert (call('state'), history()) == before, 'Rejected command changed memory or history'
        return request

    def cli(*arguments, okay=True):
        global calls
        calls += 1
        result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                                text=True, encoding='utf-8', timeout=60)
        assert (result.returncode == 0) == okay, (arguments, result.stdout, result.stderr)
        return result

    def invoice(key, date='2026-10-13'):
        return dict(id=key, source='synthetic-m9', product_id='VLV-100', customer_id='C-ATLAS',
                    date=date, quantity=3000, unit='each', currency='USD',
                    invoice_total=20000, cost_total=10001)

    def proposal(key, original='T1', quantity=1000):
        return dict(id=key, original_transaction_id=original, effective_date='2027-02-01',
                    reason='=Synthetic quantity return', quantity=quantity)

    def credit(key, original='T1', quantity=1000):
        p = proposal(key, original, quantity)
        preview = call('preview_credit', dict(proposal=p, owner='Morgan'))
        before = call('state')
        assert preview['version'] == 1 and not preview['review']
        assert call('state') == before
        execute('credit_create', dict(proposal=p))
        execute('credit_submit', dict(id=key, version=1))
        retry = execute('credit_decide', dict(id=key, version=2, approve=True, note='Independent quantity allocation review'), role='finance', name='Jordan')
        assert call('execute', retry)['replayed']
        return preview

    def accrue(key):
        execute('rebate_accrue', dict(id=key, agreement_id='A', version=3))

    def settlement(key, accrual):
        execute('settlement_create', dict(id=key, accrual_id=accrual, reason='Synthetic rebate true-up'))
        execute('settlement_submit', dict(id=key, version=1))
        execute('settlement_decide', dict(id=key, version=2, approve=True, note='Independent current-basis review'), role='finance', name='Jordan')

    def close_terms(key, currency='USD'):
        return dict(id=key, start='2026-10-01', end='2026-11-01', currency=currency,
                    reason='Synthetic nominal invoice-basis freeze')

    def close(key, currency='USD'):
        terms = close_terms(key, currency)
        preview = call('preview_period_close', dict(terms=terms, owner='Morgan'))
        execute('period_close_create', dict(terms=terms))
        execute('period_close_submit', dict(id=key, version=1))
        execute('period_close_decide', dict(id=key, version=2, approve=True, note='Nominal scope and exact IDs reviewed'), role='finance', name='Jordan')
        return preview

    request = dict(start='2026-10-01', end='2026-11-01', currency='EUR')
    try:
        execute('seed_demo', role='admin', name='Casey')
        rates = ('id,kind,product_id,from,to,start,end,numerator,denominator\n'
                 'FX,fx,,USD,EUR,2026-10-01,2026-11-01,1,2\n'
                 'BACK,fx,,EUR,USD,2026-10-01,2026-11-01,2,1\n'
                 'BOX,uom,VLV-100,box,each,2026-10-01,2026-11-01,12,1\n')
        execute('conversion_import', dict(id='RATES', name='Synthetic direct factors', csv=rates))
        execute('conversion_submit', dict(id='RATES', version=1))
        execute('conversion_decide', dict(id='RATES', version=2, approve=True, note='Synthetic factors reviewed'), role='approver', name='Riley')
        execute('agreement_create', dict(terms=dict(id='A', name='Synthetic original-period rebate',
                customer_id='C-ATLAS', currency='USD', start='2026-01-01', end='2027-01-01',
                tiers=[dict(threshold=0, rate=200), dict(threshold=10000, rate=500)])))
        execute('agreement_submit', dict(id='A', version=1))
        execute('agreement_decide', dict(id='A', version=2, approve=True, note='Original-period terms reviewed'), role='approver', name='Riley')
        execute('transaction_add', invoice('T1'))
        accrue('A0')
        settlement('S0', 'A0')
        execute('report_convert', dict(id='R0', request=request))
        old_report = call('export_report', dict(id='R0'))['csv']
        old_settlement = call('export_settlement', dict(id='S0'))['csv']
        snapshots = history()
        p1 = credit('C1')
        assert (p1['invoice'], p1['cost'], p1['prior_quantity']) == (6667, 3334, 0)
        execute('credit_create', dict(proposal=proposal('TOO-MUCH', quantity=2001)), okay=False)
        execute('correction_create', dict(proposal=dict(id='FULL', original_transaction_id='T1',
                effective_date='2027-02-01', reason='Cannot mix credits with full reversal', replacement=None)), okay=False)
        execute('settlement_create', dict(id='STALE', accrual_id='A0', reason='Old gross basis'), okay=False)
        p2 = credit('C2')
        assert (p2['invoice'], p2['cost'], p2['prior_credit_ids']) == (6666, 3333, ['C1'])
        assert call('reconciliation', dict(agreement_id='A'))['target_rebate'] == 133
        p3 = credit('C3')
        assert (p3['invoice'], p3['cost']) == (6667, 3334)
        group = call('analytics')[0]
        assert (group['invoice'], group['cost'], group['credit_count'], group['fully_credited_count'], group['transaction_count']) == (0, 0, 3, 1, 0)
        report = call('converted_actuals', request)
        assert [c['invoice'] for c in report['rows'][0]['credits']] == [3333, 3334, 3333]
        assert [c['cost'] for c in report['rows'][0]['credits']] == [1667, 1667, 1667]
        assert (report['groups'][0]['invoice'], report['groups'][0]['cost']) == (0, 0)
        accrue('A1')
        settlement('S1', 'A1')
        assert call('reconciliation', dict(agreement_id='A'))['remaining_adjustment'] == 0
        assert call('state')['settlements']['S1']['adjustment'] == -1000
        assert call('state')['accruals']['A1']['credit_ids'] == ['C1', 'C2', 'C3']
        assert call('export_settlement', dict(id='S0'))['csv'] == old_settlement
        assert 'credit_ids' in call('export_settlement', dict(id='S1'))['csv']
        assert call('export_report', dict(id='R0'))['csv'] == old_report
        assert all(history()[k] == v for k, v in snapshots.items())
        execute('report_convert', dict(id='R1', request=request))
        exported_report = call('export_report', dict(id='R1'))['csv']
        row = next(csv.DictReader(io.StringIO(exported_report)))
        assert (row['net_invoice'], row['net_cost']) == ('0.00', '0.00')
        assert len(json.loads(row['credit_evidence_json'])) == 3
        # Fully approved EUR booking preserves native provenance under a half-quantity credit.
        execute('quote_create', dict(id='Q', title='Synthetic EUR boxed order', customer_id='C-ATLAS', date='2026-10-13'), role='sales', name='Alex')
        execute('quote_add_line', dict(id='Q', version=1, request=dict(product_id='VLV-100', customer_id='C-ATLAS', date='2026-10-13', quantity=1000, discount=0, rebate=0, conversion=dict(currency='EUR', unit='box'))), role='sales', name='Alex')
        execute('quote_submit', dict(id='Q', version=2), role='sales', name='Alex')
        execute('quote_decide', dict(id='Q', version=3, approve=True, note='Approved converted order'), role='approver', name='Riley')
        execute('quote_book', dict(request=dict(id='B', quote_id='Q', quote_version=4, invoice_date='2026-10-15', source='synthetic-m9-booking')))
        booked = call('state')['transactions']['B-L001']
        native = credit('CB', 'B-L001', 500)
        assert native['booking']['native_quantity'] == 6000
        assert native['booking']['native_basis']['quantity'] == 12000
        converted = call('converted_actuals', dict(request, currency='USD'))
        book_row = next(r for r in converted['rows'] if r['native']['id'] == 'B-L001')
        assert book_row['booking']['rate_knowledge_revision'] == native['original_revision'] - 1
        assert book_row['credits'][0]['invoice'] == booked['invoice_total']
        # Close competes with a pending credit; rejecting it restores the exact close basis.
        terms = close_terms('EUR-CLOSE', 'EUR')
        execute('period_close_create', dict(terms=terms))
        execute('period_close_submit', dict(id='EUR-CLOSE', version=1))
        execute('credit_create', dict(proposal=proposal('PENDING', 'B-L001', 100)))
        execute('period_close_decide', dict(id='EUR-CLOSE', version=2, approve=True, note='Blocked by unresolved credit'), role='finance', name='Jordan', okay=False)
        execute('credit_withdraw', dict(id='PENDING', version=1))
        execute('period_close_decide', dict(id='EUR-CLOSE', version=2, approve=True, note='Unresolved credit withdrawn; exact basis unchanged'), role='finance', name='Jordan')
        execute('credit_create', dict(proposal=proposal('LATE', 'B-L001', 500)), okay=False)
        close('USD-CLOSE')
        execute('transaction_add', invoice('BACKDATED'), okay=False)
        execute('transactions_import', dict(csv='id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\nOUT,synthetic,VLV-100,C-ATLAS,2026-11-01,1,each,USD,1,0\nIN,synthetic,VLV-100,C-ATLAS,2026-10-31,1,each,USD,1,0\n'), okay=False)
        execute('transaction_add', dict(invoice('ADJACENT', '2026-11-01'), invoice_total=0, cost_total=0))
        execute('period_close_create', dict(terms=close_terms('OVERLAP')), okay=False)
        # Public CLI/API exports, reopen, exact retry and archive roundtrip preserve evidence.
        frozen = call('state')
        credit_csv = call('export_credit', dict(id='CB'))['csv']
        close_csv = call('export_period_close', dict(id='EUR-CLOSE'))['csv']
        assert json.loads(next(csv.DictReader(io.StringIO(credit_csv)))['evidence_json']) == frozen['credits']['CB']
        assert json.loads(next(csv.DictReader(io.StringIO(close_csv)))['evidence_json']) == frozen['period_closes']['EUR-CLOSE']
        api.close()
        credit_path, close_path = root/'credit.csv', root/'close.csv'
        cli('--data', data, 'export-credit', 'CB', credit_path)
        cli('--data', data, 'export-period-close', 'EUR-CLOSE', close_path)
        assert credit_path.read_bytes() == credit_csv.encode()
        assert close_path.read_bytes() == close_csv.encode()
        cli('--data', data, 'export-credit', 'CB', credit_path, okay=False)
        archive = root/'history.clbak'
        cli('--data', data, 'backup', archive)
        restored = root/'restored'
        cli('restore', archive, restored)
        assert json.loads(cli('--data', restored, 'state').stdout) == frozen
        assert {p.name:p.read_bytes() for p in data.glob('*.json')} == {p.name:p.read_bytes() for p in restored.glob('*.json')}
        api = start()
        assert call('state') == frozen
        assert call('export_report', dict(id='R0'))['csv'] == old_report
        print(json.dumps(dict(ok=True, calls=calls, revision=revision,
                              verified='quantity allocation, original-rate/native evidence, rebates, close races, rollback, exports, reopen and exact archive'), sort_keys=True))
    finally:
        api.close()
