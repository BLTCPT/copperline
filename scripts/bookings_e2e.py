#!/usr/bin/env python3
"""Approved converted quote → immutable actuals → reversal/reporting, via real processes."""
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
calls = 0
revision = 0
serial = 0

with tempfile.TemporaryDirectory(prefix='copperline-m8-flow-') as temporary:
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
        request = dict(request_id=f'm8-{calls+1}', expected_revision=revision,
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

    def approve_book(key, content):
        execute('conversion_import', dict(id=key, name='Synthetic M8 declared rates', csv=content))
        execute('conversion_submit', dict(id=key, version=1))
        execute('conversion_decide', dict(id=key, version=2, approve=True, note='Independent rate review'),
                role='approver', name='Riley')

    def contract(edition, start, price):
        terms = dict(id='CP', edition=edition, name='Synthetic contract editions', customer_id='C-ATLAS',
                     currency='USD', start=start, end='2027-01-01', priority=10,
                     rows=[dict(id='ROW', product_id='VLV-100', unit='each', min_quantity=1000,
                                max_quantity=None, basis=dict(kind='fixed', invoice_unit=price))])
        execute('contract_create', dict(terms=terms))
        execute('contract_submit', dict(id='CP', edition=edition, version=1))
        execute('contract_decide', dict(id='CP', edition=edition, version=2, approve=True,
                                        note='Independent contract review'), role='approver', name='Riley')

    def raw_invoice(key):
        return dict(id=key, source='synthetic-m8', product_id='VLV-100', customer_id='C-ATLAS',
                    date='2026-09-01', quantity=1000, unit='each', currency='USD',
                    invoice_total=10000, cost_total=6000)

    def reverse(key, original):
        execute('correction_create', dict(proposal=dict(id=key, original_transaction_id=original,
                effective_date='2026-11-20', reason='Synthetic complete booked-row reversal', replacement=None)))
        execute('correction_submit', dict(id=key, version=1))
        execute('correction_decide', dict(id=key, version=2, approve=True,
                note='Original immutable booking reviewed'), role='finance', name='Jordan')

    try:
        execute('seed_demo', role='admin', name='Casey')
        rates = Path('fixtures/conversions.csv').read_text(encoding='utf-8')
        rates += 'EUR-USD-NOV,fx,,EUR,USD,2026-11-01,2026-12-01,5,4\n'
        approve_book('RATES', rates)
        contract(1, '2026-10-01', 9000)
        execute('quote_create', dict(id='Q', title='Synthetic full EUR box order', customer_id='C-ATLAS',
                                    date='2026-10-13'), role='sales', name='Alex')
        price = dict(product_id='VLV-100', customer_id='C-ATLAS', date='2026-10-13', quantity=1000,
                     discount=100, rebate=300, source=dict(kind='customer_contract'),
                     conversion=dict(currency='EUR', unit='box'))
        for version, quantity in [(1, 1000), (2, 2000)]:
            execute('quote_add_line', dict(id='Q', version=version, request={**price, 'quantity':quantity}),
                    role='sales', name='Alex')
        request = dict(id='BOOK', quote_id='Q', quote_version=5, invoice_date='2026-11-13', source='synthetic-m8')
        execute('quote_book', dict(request=request), okay=False)  # Draft is never bookable.
        execute('quote_submit', dict(id='Q', version=3), role='sales', name='Alex')
        execute('quote_book', dict(request=request), okay=False)  # Pending is never bookable.
        execute('quote_decide', dict(id='Q', version=4, approve=True, note='Frozen commercial terms'),
                role='approver', name='Riley')
        approved = call('state')['quotes']['Q']
        frozen_quote_csv = call('export_quote', dict(id='Q'))['csv']
        contract(2, '2026-11-01', 8500)
        current_price = call('evaluate', {**price, 'date':'2026-11-13'})
        assert current_price['invoice_unit'] != approved['lines'][0]['invoice_unit']
        preview = call('preview_quote_booking', request)
        assert [l['booked']['invoice_total'] for l in preview] == [96228, 192456]
        assert preview[0]['native_basis']['invoice_total'] == 106920
        assert preview[0]['booked']['currency'] == 'EUR' and preview[0]['booked']['unit'] == 'box'
        assert preview[0]['native_basis']['currency'] == 'USD' and preview[0]['native_basis']['unit'] == 'each'
        execute('quote_book', dict(request=request), role='sales', name='Alex', okay=False)
        execute('quote_book', dict(request={**request, 'quote_version':4}), okay=False)
        execute('quote_book', dict(request={**request, 'invoice_date':'2026-10-12'}), okay=False)
        execute('quote_book', dict(request={**request, 'quantity':1000}), okay=False)
        execute('transaction_add', {**preview[0]['booked'], 'id':'RAW-NONNATIVE'}, okay=False)
        execute('transaction_add', raw_invoice('ORIGINAL'))
        execute('correction_create', dict(proposal=dict(id='RESERVE', original_transaction_id='ORIGINAL',
                effective_date='2026-11-13', reason='Synthetic reserved-ID collision', replacement=raw_invoice('BOOK-L002'))))
        execute('quote_book', dict(request=request), okay=False)
        execute('correction_withdraw', dict(id='RESERVE', version=1))
        api.close()
        preview_cli = cli('--data', data, 'preview-booking', json.dumps(request))
        assert json.loads(preview_cli.stdout) == preview
        retry = dict(request_id='m8-original-booking', expected_revision=revision,
                     actor=dict(name='Morgan', role='analyst'), command='quote_book', params=dict(request=request))
        result = json.loads(cli('--data', data, 'execute', json.dumps(retry)).stdout)
        revision = result['revision']
        assert not result['replayed']
        assert json.loads(cli('--data', data, 'execute', json.dumps(retry)).stdout)['replayed']
        api = start()
        state = call('state')
        booking = state['quote_bookings']['BOOK']
        assert booking['lines'] == preview and booking['approval_cycle']['lines'] == approved['lines']
        assert booking['quote_contributors'] == ['Alex']
        assert call('execute', retry)['replayed']
        assert len(state['transactions']) == 3
        execute('quote_book', dict(request={**request, 'id':'OTHER'}), okay=False)
        booking_csv = call('export_booking', dict(id='BOOK'))['csv']
        exported = list(csv.DictReader(io.StringIO(booking_csv)))
        assert len(exported) == 2 and exported[0]['booked_currency'] == 'EUR'
        assert json.loads(exported[0]['native_nonposted_basis_json'])['invoice_total'] == 106920
        period = dict(start='2026-11-01', end='2026-12-01', currency='EUR')
        original_actuals = call('converted_actuals', period)
        assert original_actuals['groups'][0]['invoice'] == 288684
        assert original_actuals['groups'][0]['cost'] == 194400
        usd = call('converted_actuals', {**period, 'currency':'USD'})
        assert usd['groups'][0]['invoice'] == 360855  # Explicit invoice-date EUR→USD, not native basis.
        assert usd['rows'][0]['fx']['captured_revision'] == booking['created_revision'] - 1
        assert usd['rows'][0]['booking']['native_basis']['invoice_total'] == 106920
        execute('report_convert', dict(id='BEFORE', request={**period, 'currency':'USD'}))
        report_csv = call('export_report', dict(id='BEFORE'))['csv']
        report_rows = list(csv.DictReader(io.StringIO(report_csv)))
        assert report_rows[0]['recorded_currency'] == 'EUR' and 'native_currency' not in report_rows[0]
        assert json.loads(report_rows[0]['booking_evidence_json'])['booking_id'] == 'BOOK'
        late = rates.splitlines()[0]+'\nLATE,fx,,EUR,GBP,2026-11-01,2026-12-01,4,5\n'
        approve_book('LATE', late)
        call('converted_actuals', {**period, 'currency':'GBP'}, okay=False)
        execute('report_convert', dict(id='NO-BACKFILL', request={**period, 'currency':'GBP'}), okay=False)
        later_usd = call('converted_actuals', {**period, 'currency':'USD'})
        assert later_usd['rows'] == usd['rows'] and later_usd['groups'] == usd['groups']
        execute('quote_revise', dict(id='Q', version=5), role='sales', name='Alex')
        execute('quote_reprice', dict(id='Q', version=6), role='sales', name='Alex')
        execute('quote_submit', dict(id='Q', version=7), role='sales', name='Alex')
        execute('quote_decide', dict(id='Q', version=8, approve=True, note='New quote review'), role='approver', name='Riley')
        execute('quote_book', dict(request={**request, 'id':'REVISED', 'quote_version':9}), okay=False)
        assert call('export_booking', dict(id='BOOK'))['csv'] == booking_csv
        assert call('state')['quote_bookings']['BOOK'] == booking
        execute('correction_create', dict(proposal=dict(id='INVALID-REPLACE', original_transaction_id='BOOK-L001',
                effective_date='2026-11-20', reason='Replacement forbidden for booked evidence', replacement=raw_invoice('UNRELATED'))), okay=False)
        terms = dict(id='REBATE', name='Separate synthetic actual rebate', customer_id='C-ATLAS', currency='EUR',
                     start='2026-11-01', end='2026-12-01', tiers=[dict(threshold=0, rate=100)])
        execute('agreement_create', dict(terms=terms))
        execute('agreement_submit', dict(id='REBATE', version=1))
        execute('agreement_decide', dict(id='REBATE', version=2, approve=True, note='Separate actual rebate'), role='approver', name='Riley')
        execute('rebate_accrue', dict(id='ACCRUAL-BEFORE', agreement_id='REBATE', version=3))
        assert call('state')['accruals']['ACCRUAL-BEFORE']['rebate'] == 2887
        reverse('REV-1', 'BOOK-L001')
        after = call('converted_actuals', {**period, 'currency':'USD'})
        assert after['groups'][0]['invoice'] == 240570 and after['groups'][0]['reversed_invoice'] == 120285
        execute('rebate_accrue', dict(id='ACCRUAL-AFTER', agreement_id='REBATE', version=3))
        assert call('state')['accruals']['ACCRUAL-AFTER']['rebate'] == 1925
        execute('settlement_create', dict(id='SETTLE', accrual_id='ACCRUAL-AFTER', reason='Synthetic local recognition'))
        execute('settlement_submit', dict(id='SETTLE', version=1))
        execute('settlement_decide', dict(id='SETTLE', version=2, approve=True, note='Quote author cannot review'), role='finance', name='Alex', okay=False)
        execute('settlement_decide', dict(id='SETTLE', version=2, approve=True, note='Independent recognition'), role='finance', name='Jordan')
        reverse('REV-2', 'BOOK-L002')
        zero = call('converted_actuals', {**period, 'currency':'USD'})
        assert zero['groups'][0]['invoice'] == 0 and zero['groups'][0]['cost'] == 0
        execute('quote_book', dict(request={**request, 'id':'AFTER-REVERSAL', 'quote_version':9}), okay=False)
        assert call('export_report', dict(id='BEFORE'))['csv'] == report_csv
        assert call('export_booking', dict(id='BOOK'))['csv'] == booking_csv
        before_reopen = call('state')
        original_history = history()
        api.close()
        output = root / 'booking.csv'
        cli('--data', data, 'export-booking', 'BOOK', output)
        assert output.read_bytes() == booking_csv.encode('utf-8')
        cli('--data', data, 'export-booking', 'BOOK', output, okay=False)
        assert output.read_bytes() == booking_csv.encode('utf-8')
        archive = root / 'booked.clbak'
        cli('--data', data, 'backup', archive)
        restored = root / 'restored'
        cli('restore', archive, restored)
        assert json.loads(cli('--data', restored, 'state').stdout) == before_reopen
        assert json.loads(cli('--data', restored, 'execute', json.dumps(retry)).stdout)['replayed']
        api = start()
        assert call('state') == before_reopen and history() == original_history
        assert call('execute', retry)['replayed']
        assert call('export_booking', dict(id='BOOK'))['csv'] == booking_csv
        assert call('export_report', dict(id='BEFORE'))['csv'] == report_csv
        print(json.dumps(dict(result='PASS', process_and_api_calls=calls, committed_revisions=revision,
            booked_rows=2, booked_invoice_cents=288684, reporting_gross_usd_cents=360855,
            booking_csv_sha256=hashlib.sha256(booking_csv.encode()).hexdigest(),
            original_quote_csv_sha256=hashlib.sha256(frozen_quote_csv.encode()).hexdigest(),
            verified=['full converted quote atomic booking', 'CLI/API exact preview and shared command',
                'draft/pending/stale/role/partial-input rejection preserves state and disk',
                'reserved invoice ID and duplicate booking protection', 'strict raw native ingestion',
                'superseded contract and dated FX do not reprice the frozen quote',
                'booked ledger versus non-posted native basis', 'booking-time direct reporting rates; no later backfill',
                'post-booking quote revision preserves frozen evidence', 'full reversals and original reporting cancellation',
                'independent actual rebates and quote-contributor settlement exclusion',
                'immutable report/export and no-overwrite CLI export', 'receipt replay, reopen and complete archive restoration']), indent=2))
    finally:
        api.abort()
