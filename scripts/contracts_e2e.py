#!/usr/bin/env python3
"""Synthetic contract intervals, source snapshots and atomic quote replacement."""
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
with tempfile.TemporaryDirectory(prefix='copperline-m5-e2e-') as directory:
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
        result = json.loads(line)
        assert result['ok'] == okay, result
        return result.get('result') if okay else result
    def execute(command, params=None, role='analyst', name='Morgan', okay=True, request=None):
        global revision
        request = request or {'request_id': f'm5-{count+1}', 'expected_revision': revision,
                              'actor': {'name': name, 'role': role}, 'command': command,
                              **({'params': params} if params is not None else {})}
        before = call('state') if not okay else None
        result = call('execute', request, okay)
        if okay:
            revision = result['revision']
        else:
            assert call('state') == before, 'Rejected command changed state'
        return request, result
    def price(date='2026-11-15', quantity=10000, source=None):
        return {'product_id': 'VLV-100', 'customer_id': 'C-ATLAS', 'date': date,
                'quantity': quantity, 'discount': 0, 'rebate': 0,
                'source': source or {'kind': 'customer_contract'}}
    def fixed(row_id, minimum, maximum, amount):
        return {'id': row_id, 'product_id': 'VLV-100', 'unit': 'each', 'min_quantity': minimum,
                'max_quantity': maximum, 'basis': {'kind': 'fixed', 'invoice_unit': amount}}
    def terms(id='CP', edition=1, start='2026-10-01', end='2027-01-01', priority=10, rows=None):
        return {'id': id, 'edition': edition, 'name': '=Synthetic contract – étalon', 'customer_id': 'C-ATLAS',
                'currency': 'USD', 'start': start, 'end': end, 'priority': priority,
                'rows': rows or [fixed('BULK',10000,None,8500)]}
    def contract(t, approve=True):
        execute('contract_create', {'terms': t})
        execute('contract_submit', {'id': t['id'], 'edition': t['edition'], 'version': 1})
        if approve:
            return execute('contract_decide', {'id': t['id'], 'edition': t['edition'], 'version': 2,
                           'approve': True, 'note': 'Independent commercial terms review'},
                           role='approver', name='Riley')
    try:
        execute('seed_demo', role='admin', name='Casey')
        origin = price('2026-10-13',10000,{'kind':'grid','id':'G','approval_revision':4})
        live = dict(origin, discount=1000);live.pop('source')
        execute('grid_calculate', {'id':'G','version':0,'request':live})
        execute('grid_submit', {'id':'G','version':1})
        execute('grid_decide', {'id':'G','version':2,'approve':True,'note':'Synthetic origin reviewed'},role='approver',name='Riley')
        source_row = fixed('BULK',10000,None,9000)
        source_row['basis'] = {'kind':'approved_source','request':origin}
        first = terms(rows=[fixed('RETAIL',1000,10000,9500),source_row])
        contract(first,approve=False)
        execute('contract_decide',{'id':'CP','edition':1,'version':2,'approve':True,'note':'Own terms'},role='approver',okay=False)
        execute('contract_decide',{'id':'CP','edition':1,'version':2,'approve':True,'note':'Wrong role'},role='admin',name='Casey',okay=False)
        execute('contract_decide',{'id':'CP','edition':1,'version':2,'approve':True,'note':'Independent terms'},role='approver',name='Riley')
        for date,quantity,unit in [('2026-10-01',1000,9500),('2026-12-31',9999,9500),('2026-11-15',10000,9000)]:
            assert call('evaluate',price(date,quantity))['rule_unit']==unit
        for date,quantity in [('2026-09-30',1000),('2027-01-01',1000),('2026-10-01',999)]:
            call('evaluate',price(date,quantity),okay=False)
        execute('quote_create',{'id':'Q','title':'Synthetic contract quote','customer_id':'C-ATLAS','date':'2026-11-15'},role='sales',name='Alex')
        execute('quote_add_line',{'id':'Q','version':1,'request':price()},role='sales',name='Alex')
        execute('quote_submit',{'id':'Q','version':2},role='sales',name='Alex')
        execute('quote_decide',{'id':'Q','version':3,'approve':True,'note':'Independent quote'},role='approver',name='Riley')
        old_quote=call('export_quote',{'id':'Q'})['csv']
        old_contract=call('export_contract',{'id':'CP','edition':1})['csv']
        row=next(csv.DictReader(io.StringIO(old_quote)))
        assert row['contract_selection']=='automatic' and row['contract_row']=='BULK'
        assert row['contract_start']=='2026-10-01' and row['contract_end_exclusive']=='2027-01-01'
        contract_rows=list(csv.DictReader(io.StringIO(old_contract)))
        assert contract_rows[0]['name']=="'=Synthetic contract – étalon"
        assert contract_rows[1]['origin']=='Grid G · approval 4'
        contract(terms(edition=2,start='2026-11-01',end='2026-12-01'))
        assert call('evaluate',price())['rule_unit']==8500
        call('evaluate',price(source={'kind':'contract','id':'CP','edition':1}),okay=False)
        call('evaluate',price('2026-12-01'),okay=False)  # no older edition resurrection
        assert call('export_quote',{'id':'Q'})['csv']==old_quote
        assert call('export_contract',{'id':'CP','edition':1})['csv']==old_contract
        contract(terms(edition=3,start='2026-12-01',rows=[fixed('SMALL',1000,5000,9600)]))
        call('evaluate',price('2026-12-15',10000),okay=False)
        assert call('evaluate',price('2026-12-15',3000))['rule_unit']==9600
        contract(terms(id='HIGH',start='2026-11-01',end='2026-12-01',priority=20,rows=[fixed('BULK',10000,None,8200)]))
        assert call('evaluate',price())['rule_unit']==8200
        assert call('evaluate',price(source={'kind':'contract','id':'CP','edition':2}))['rule_unit']==8500
        conflict=terms(id='CONFLICT',start='2026-11-15',end='2026-11-20',priority=20)
        contract(conflict,approve=False)
        execute('contract_decide',{'id':'CONFLICT','edition':1,'version':2,'approve':True,'note':'Overlaps'},role='approver',name='Riley',okay=False)
        execute('contract_withdraw',{'id':'CONFLICT','edition':1,'version':2})
        bad=terms(id='BAD-CURRENCY');bad['currency']='EUR'
        execute('contract_create',{'terms':bad},okay=False)
        bad=terms(id='BAD-UNIT');bad['rows'][0]['unit']='box'
        execute('contract_create',{'terms':bad},okay=False)
        execute('quote_revise',{'id':'Q','version':4},role='sales',name='Alex')
        execute('quote_replace_line',{'id':'Q','version':5,'index':0,'request':price('2026-12-15')},role='sales',name='Alex',okay=False)
        execute('quote_replace_line',{'id':'Q','version':5,'index':0,'request':price()},role='sales',name='Alex')
        quote=call('state')['quotes']['Q'];assert len(quote['lines'])==1 and quote['lines'][0]['invoice_total']==82000
        execute('quote_submit',{'id':'Q','version':6},role='sales',name='Alex')
        retry,_=execute('quote_decide',{'id':'Q','version':7,'approve':True,'note':'Replacement reviewed'},role='approver',name='Riley')
        assert call('state')['quotes']['Q']['governance']['cycles'][0]['lines'][0]['invoice_total']==90000
        # A later grid approval cannot rewrite or invalidate an already approved contract origin.
        execute('grid_calculate',{'id':'G','version':3,'request':dict(live,discount=0)})
        execute('grid_submit',{'id':'G','version':4})
        execute('grid_decide',{'id':'G','version':5,'approve':True,'note':'New origin price'},role='approver',name='Riley')
        assert call('evaluate',price('2026-10-15',10000,{'kind':'contract','id':'CP','edition':1}))['rule_unit']==9000
        assert call('export_contract',{'id':'CP','edition':1})['csv']==old_contract
        final_export=call('export_quote',{'id':'Q'})['csv']
        before=call('state')
        process.stdin.close();assert process.wait(timeout=10)==0
        process=start();assert call('state')==before
        assert call('execute',retry)['replayed']
        assert call('export_quote',{'id':'Q'})['csv']==final_export
        assert call('export_contract',{'id':'CP','edition':1})['csv']==old_contract
        process.stdin.close();assert process.wait(timeout=10)==0
        output=Path(directory)/'contract.csv'
        first_export=subprocess.run([binary,'--data',directory,'export-contract','CP','1',str(output)],capture_output=True,text=True, encoding="utf-8")
        assert first_export.returncode==0,first_export.stderr
        assert output.read_bytes()==old_contract.encode("utf-8")
        duplicate=subprocess.run([binary,'--data',directory,'export-contract','CP','1',str(output)],capture_output=True,text=True, encoding="utf-8")
        assert duplicate.returncode!=0 and output.read_bytes()==old_contract.encode("utf-8")
        print(json.dumps({'result':'PASS','api_requests':count,'committed_revisions':revision,
              'verified':['half-open date and quantity boundaries','independent source-contributor review','frozen grid origins',
              'edition supersession without resurrection','explicit priority and overlap rejection','no implicit currency/UOM conversion',
              'automatic versus exact source selection','atomic quote-line replacement','immutable approval cycles',
              'restart and exact retry','byte-stable historical exports','UTF-8 and formula-safe contract CSV','no-overwrite CLI export']},indent=2))
    finally:
        if process.poll() is None:
            process.kill();process.wait(timeout=10)
