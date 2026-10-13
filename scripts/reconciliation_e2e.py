#!/usr/bin/env python3
"""Synthetic approved-correction and rebate reconciliation workflow with real restart."""
import argparse
import csv
import io
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/copperline-cli')
args = parser.parse_args()
binary = str(Path(args.binary).resolve())
count = 0
revision = 0

with tempfile.TemporaryDirectory(prefix='copperline-m4-e2e-') as directory:
    def start():
        return subprocess.Popen([binary, '--data', directory, 'api'], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")

    process = start()
    def call(method, params=None, okay=True):
        global count
        count += 1
        process.stdin.write(json.dumps({'method': method, 'params': params}) + '\n')
        process.stdin.flush()
        line = process.stdout.readline()
        assert line, process.stderr.read()
        response = json.loads(line)
        assert response['ok'] == okay, response
        return response.get('result') if okay else response

    def execute(command, params=None, role='analyst', name='Morgan', okay=True, request=None):
        global revision
        request = request or {'request_id': f'm4-{count+1}', 'expected_revision': revision,
                              'actor': {'name': name, 'role': role}, 'command': command,
                              **({'params': params} if params is not None else {})}
        before = call('state') if not okay else None
        result = call('execute', request, okay=okay)
        if okay:
            revision = result['revision']
        else:
            assert call('state') == before, 'Rejected mutation changed state'
        return request, result

    def invoice(id, amount, cost):
        return {'id': id, 'source': 'synthetic-m4', 'product_id': 'VLV-100',
                'customer_id': 'C-ATLAS', 'date': '2026-10-13', 'quantity': 10000,
                'unit': 'each', 'currency': 'USD', 'invoice_total': amount, 'cost_total': cost}

    def accrue(id):
        execute('rebate_accrue', {'id': id, 'agreement_id': 'A1', 'version': 3})

    def settlement(id, accrual, finish=True):
        execute('settlement_create', {'id': id, 'accrual_id': accrual, 'reason': '=Synthetic reconciliation'})
        execute('settlement_submit', {'id': id, 'version': 1})
        if finish:
            return execute('settlement_decide', {'id': id, 'version': 2, 'approve': True,
                           'note': 'Independent synthetic review'}, role='finance', name='Jordan')

    def correction(id, original, replacement=None):
        execute('correction_create', {'proposal': {'id': id, 'original_transaction_id': original,
                'effective_date': '2027-02-01', 'reason': 'Synthetic correction of 2026 actuals',
                'replacement': replacement}})
        execute('correction_submit', {'id': id, 'version': 1})

    try:
        execute('seed_demo', role='admin', name='Casey')
        execute('agreement_create', {'terms': {'id': 'A1', 'name': 'Synthetic retrospective rebate',
                'customer_id': 'C-ATLAS', 'currency': 'USD', 'start': '2026-01-01', 'end': '2027-01-01',
                'tiers': [{'threshold': 0, 'rate': 200}, {'threshold': 100000, 'rate': 500}]}})
        execute('agreement_submit', {'id': 'A1', 'version': 1})
        execute('agreement_decide', {'id': 'A1', 'version': 2, 'approve': True, 'note': 'Independent terms review'}, role='approver', name='Riley')
        execute('transaction_add', invoice('T1', 50000, 30000))
        execute('transaction_add', invoice('T2', 250000, 150000))
        accrue('R0')
        settlement('S0', 'R0', finish=False)
        settlement('COMPETING', 'R0', finish=False)
        execute('settlement_decide', {'id': 'S0', 'version': 2, 'approve': True, 'note': 'Wrong role'}, role='admin', name='Casey', okay=False)
        execute('settlement_decide', {'id': 'S0', 'version': 2, 'approve': True, 'note': 'Own work'}, role='finance', okay=False)
        execute('settlement_decide', {'id': 'S0', 'version': 2, 'approve': True, 'note': 'Independent review'}, role='finance', name='Jordan')
        original_export = call('export_settlement', {'id': 'S0'})['csv']
        assert next(csv.DictReader(io.StringIO(original_export)))['adjustment'] == '150.00'
        assert next(csv.DictReader(io.StringIO(original_export)))['reason'].startswith("'=")
        execute('settlement_decide', {'id': 'COMPETING', 'version': 2, 'approve': True, 'note': 'Stale baseline'}, role='finance', name='Jordan', okay=False)
        execute('settlement_withdraw', {'id': 'COMPETING', 'version': 2})
        correction('C1', 'T2')
        assert call('analytics')[0]['invoice'] == 300000, 'Pending correction altered actuals'
        execute('correction_decide', {'id': 'C1', 'version': 2, 'approve': True, 'note': 'Own work'}, role='finance', okay=False)
        execute('correction_decide', {'id': 'C1', 'version': 2, 'approve': True, 'note': 'Finance full reversal'}, role='finance', name='Jordan')
        assert call('analytics')[0]['invoice'] == 50000
        position = call('reconciliation', {'agreement_id': 'A1'})
        assert (position['target_rebate'], position['recognized_rebate'], position['remaining_adjustment']) == (1000, 15000, -14000)
        execute('settlement_create', {'id': 'STALE', 'accrual_id': 'R0', 'reason': 'Old gross basis'}, okay=False)
        accrue('R1')
        retry, _ = settlement('S1', 'R1')
        _, replay = execute('', request=retry)
        assert replay['replayed']
        refund_export = call('export_settlement', {'id': 'S1'})['csv']
        assert next(csv.DictReader(io.StringIO(refund_export)))['adjustment'] == '-140.00'
        correction('C2', 'T1', invoice('T1-R', 40000, 24000))
        execute('transaction_add', invoice('T1-R', 40000, 24000), okay=False)
        execute('correction_decide', {'id': 'C2', 'version': 2, 'approve': True, 'note': 'Finance replacement'}, role='finance', name='Jordan')
        accrue('R2')
        settlement('S2', 'R2')
        assert call('state')['settlements']['S2']['adjustment'] == -200
        correction('C3', 'T1-R')
        execute('correction_decide', {'id': 'C3', 'version': 2, 'approve': True, 'note': 'Reverse corrected invoice'}, role='finance', name='Jordan')
        accrue('R3')
        settlement('S3', 'R3')
        position = call('reconciliation', {'agreement_id': 'A1'})
        assert (position['target_rebate'], position['recognized_rebate'], position['remaining_adjustment']) == (0, 0, 0)
        assert call('state')['settlements']['S3']['adjustment'] == -800
        execute('settlement_create', {'id': 'DUPLICATE', 'accrual_id': 'R3', 'reason': 'Nothing due'}, okay=False)
        assert call('export_settlement', {'id': 'S0'})['csv'] == original_export
        assert call('export_settlement', {'id': 'S1'})['csv'] == refund_export
        state = call('state')
        assert len(state['transactions']) == 3 and state['accruals']['R0']['rebate'] == 15000
        assert call('analytics')[0]['invoice'] == 0 and call('analytics')[0]['margin_bps'] is None
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        process = start()
        assert call('state') == state
        assert call('export_settlement', {'id': 'S1'})['csv'] == refund_export
        _, replay = execute('', request=retry)
        assert replay['replayed']
    finally:
        if process.poll() is None:
            process.stdin.close()
            assert process.wait(timeout=10) == 0
    export_path = str(Path(directory, 'approved-refund.csv'))
    exported = subprocess.run([binary, '--data', directory, 'export-settlement', 'S1', export_path], capture_output=True, text=True, encoding="utf-8", timeout=10)
    assert exported.returncode == 0, exported.stderr
    assert Path(export_path).read_text(encoding="utf-8") == refund_export
    duplicate = subprocess.run([binary, '--data', directory, 'export-settlement', 'S1', export_path], capture_output=True, text=True, encoding="utf-8", timeout=10)
    assert duplicate.returncode != 0 and Path(export_path).read_text(encoding="utf-8") == refund_export

print(json.dumps({'result': 'PASS', 'api_requests': count, 'committed_revisions': state['revision'],
                  'settlement_adjustments_usd': ['150.00', '-140.00', '-2.00', '-8.00'],
                  'final_recognized_rebate': '0.00',
                  'verified': ['independent Finance review', 'atomic full reversal and replacement',
                               'reserved replacement IDs', 'original period restatement',
                               'retrospective tier true-up', 'competing baseline rejection',
                               'zero entitlement refund', 'non-additive accrual snapshots',
                               'exact retry after restart', 'frozen CSV exports',
                               'numeric negative adjustments and formula-safe text', 'no overwrite export']}, indent=2))
