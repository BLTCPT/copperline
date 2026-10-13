#!/usr/bin/env python3
"""Explicit dated conversion, native source qualification and frozen reporting actuals."""
import argparse
import csv
import io
import json
from pathlib import Path
import subprocess
import tempfile

parser=argparse.ArgumentParser()
parser.add_argument('--binary',default='target/debug/copperline-cli')
args=parser.parse_args()
binary=str(Path(args.binary).resolve())
count=0
revision=0
with tempfile.TemporaryDirectory(prefix='copperline-m6-e2e-') as directory:
    def start():
        return subprocess.Popen([binary,'--data',directory,'api'],stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8')
    process=start()
    def call(method,params=None,okay=True):
        global count
        count+=1
        process.stdin.write(json.dumps({'method':method,'params':params})+'\n');process.stdin.flush()
        line=process.stdout.readline()
        assert line,process.stderr.read()
        result=json.loads(line)
        assert result['ok']==okay,result
        return result.get('result') if okay else result
    def execute(command,params=None,role='analyst',name='Morgan',okay=True,request=None):
        global revision
        request=request or {'request_id':f'm6-{count+1}','expected_revision':revision,
                            'actor':{'name':name,'role':role},'command':command,
                            **({'params':params} if params is not None else {})}
        before=call('state') if not okay else None
        result=call('execute',request,okay)
        if okay:revision=result['revision']
        else:assert call('state')==before,'Rejected command mutated state'
        return request,result
    def price(convert=False,date='2026-10-13',quantity=1000):
        result={'product_id':'VLV-100','customer_id':'C-ATLAS','date':date,'quantity':quantity,
                'discount':0,'rebate':0,'source':{'kind':'customer_contract'}}
        if convert:result['conversion']={'currency':'EUR','unit':'box'}
        return result
    def invoice(id,date,amount,cost):
        return {'id':id,'source':'synthetic-m6','product_id':'VLV-100','customer_id':'C-ATLAS',
                'date':date,'quantity':1000,'unit':'each','currency':'USD','invoice_total':amount,'cost_total':cost}
    try:
        execute('seed_demo',role='admin',name='Casey')
        book_csv=Path('fixtures/conversions.csv').read_text(encoding='utf-8')
        execute('conversion_import',{'id':'BOOK','name':'Synthetic conversions – étalon','csv':book_csv})
        call('evaluate',price(True),okay=False)
        execute('conversion_submit',{'id':'BOOK','version':1})
        execute('conversion_decide',{'id':'BOOK','version':2,'approve':True,'note':'Own terms'},role='approver',okay=False)
        execute('conversion_decide',{'id':'BOOK','version':2,'approve':True,'note':'No override'},role='admin',name='Casey',okay=False)
        execute('conversion_decide',{'id':'BOOK','version':2,'approve':True,'note':'Independent exact ratio review'},role='approver',name='Riley')
        terms={'id':'CP','edition':1,'name':'Synthetic native quantity bands','customer_id':'C-ATLAS','currency':'USD',
               'start':'2026-10-01','end':'2027-01-01','priority':10,'rows':[
                   {'id':'SMALL','product_id':'VLV-100','unit':'each','min_quantity':1000,'max_quantity':12000,'basis':{'kind':'fixed','invoice_unit':9500}},
                   {'id':'BOX-BULK','product_id':'VLV-100','unit':'each','min_quantity':12000,'max_quantity':None,'basis':{'kind':'fixed','invoice_unit':9000}}]}
        execute('contract_create',{'terms':terms})
        execute('contract_submit',{'id':'CP','edition':1,'version':1})
        execute('contract_decide',{'id':'CP','edition':1,'version':2,'approve':True,'note':'Bands are each'},role='approver',name='Riley')
        converted=call('evaluate',price(True))
        assert converted['currency']=='EUR' and converted['unit']=='box'
        assert converted['rule_unit']==97200 and converted['cost_unit']==64800
        assert converted['conversion']['native_quantity']==12000
        assert converted['source']['contract']['row_id']=='BOX-BULK'
        assert call('evaluate',price(True,quantity=999))['source']['contract']['row_id']=='SMALL'
        assert call('evaluate',price(True,date='2026-11-01'))['rule_unit']==86400
        call('evaluate',price(True,date='2026-12-01'),okay=False)
        bad=price(True);bad['conversion']={'currency':'GBP','unit':'box'}
        call('evaluate',bad,okay=False)
        bad=price(True);bad['conversion']={'currency':'USD','unit':'each'}
        call('evaluate',bad,okay=False)
        call('compare',{'baseline':price(),'scenario':price(True)},okay=False)
        execute('grid_calculate',{'id':'CHAIN','version':0,'request':price(True)},okay=False)
        execute('quote_create',{'id':'Q','title':'Explicit EUR box quote','customer_id':'C-ATLAS','date':'2026-10-13'},role='sales',name='Alex')
        for version in (1,2):execute('quote_add_line',{'id':'Q','version':version,'request':price(quantity=12000)},role='sales',name='Alex')
        execute('quote_replace_line',{'id':'Q','version':3,'index':0,'request':price(True)},role='sales',name='Alex',okay=False)
        retry,_=execute('quote_replace_lines',{'id':'Q','version':3,'replacements':[{'index':0,'request':price(True)},{'index':1,'request':price(True)}]},role='sales',name='Alex')
        execute('quote_submit',{'id':'Q','version':4},role='sales',name='Alex')
        execute('quote_decide',{'id':'Q','version':5,'approve':True,'note':'Frozen FX and native bands reviewed'},role='approver',name='Riley')
        quote_csv=call('export_quote',{'id':'Q'})['csv']
        rows=list(csv.DictReader(io.StringIO(quote_csv)))
        assert len(rows)==2 and rows[0]['native_quantity']=='12.000' and rows[0]['native_unit']=='each'
        assert rows[0]['conversion_rounding']=='native_unit_combined_half_away_then_extend_v1'
        fx=json.loads(rows[0]['fx_evidence_json']);uom=json.loads(rows[0]['uom_evidence_json'])
        assert fx['entry']['ratio']=={'numerator':9,'denominator':10}
        assert uom['entry']['conversion']['from_unit']=='box' and uom['book_version']==3
        execute('transaction_add',invoice('OCT','2026-10-13',10001,6001))
        execute('transaction_add',invoice('NOV','2026-11-13',10000,6000))
        period={'start':'2026-10-01','end':'2026-12-01','currency':'EUR'}
        assert call('converted_actuals',period)['groups'][0]['invoice']==17001
        execute('report_convert',{'id':'BEFORE','request':period})
        old_report=call('export_report',{'id':'BEFORE'})['csv']
        execute('correction_create',{'proposal':{'id':'REV','original_transaction_id':'OCT','effective_date':'2026-11-20','reason':'Synthetic full reversal','replacement':None}})
        execute('correction_submit',{'id':'REV','version':1})
        execute('correction_decide',{'id':'REV','version':2,'approve':True,'note':'Original invoice reviewed'},role='finance',name='Jordan')
        now=call('converted_actuals',period)
        assert now['groups'][0]['reversed_invoice']==9001 and now['groups'][0]['invoice']==8000
        assert now['groups'][0]['cost']==4800 and now['groups'][0]['margin']==3200
        execute('report_convert',{'id':'AFTER','request':period})
        assert call('export_report',{'id':'BEFORE'})['csv']==old_report
        after_report=call('export_report',{'id':'AFTER'})['csv']
        report_rows=list(csv.DictReader(io.StringIO(after_report)))
        assert next(r for r in report_rows if r['invoice_id']=='OCT')['net_invoice']=='0.00'
        execute('conversion_import',{'id':'OVERLAP','name':'Conflicting pending book','csv':book_csv})
        execute('conversion_submit',{'id':'OVERLAP','version':1})
        execute('conversion_decide',{'id':'OVERLAP','version':2,'approve':True,'note':'Conflict'},role='approver',name='Riley',okay=False)
        execute('conversion_withdraw',{'id':'OVERLAP','version':2})
        future=book_csv.splitlines()[0]+'\nFUTURE,fx,,USD,EUR,2026-12-01,2027-01-01,7,10\n'
        execute('conversion_import',{'id':'FUTURE','name':'Adjacent schedule','csv':future})
        execute('conversion_submit',{'id':'FUTURE','version':1})
        execute('conversion_decide',{'id':'FUTURE','version':2,'approve':True,'note':'Adjacent dates'},role='approver',name='Riley')
        book_export=call('export_conversion',{'id':'BOOK'})['csv']
        before=call('state')
        process.stdin.close();assert process.wait(timeout=10)==0
        process=start();assert call('state')==before
        assert call('execute',retry)['replayed']
        assert call('export_quote',{'id':'Q'})['csv']==quote_csv
        assert call('export_report',{'id':'BEFORE'})['csv']==old_report
        assert call('export_report',{'id':'AFTER'})['csv']==after_report
        process.stdin.close();assert process.wait(timeout=10)==0
        for verb,id,expected in [('export-conversion','BOOK',book_export),('export-report','AFTER',after_report),('export-quote','Q',quote_csv)]:
            output=Path(directory)/(id+'.csv')
            result=subprocess.run([binary,'--data',directory,verb,id,str(output)],capture_output=True,text=True,encoding='utf-8')
            assert result.returncode==0,result.stderr
            assert output.read_bytes()==expected.encode('utf-8')
            duplicate=subprocess.run([binary,'--data',directory,verb,id,str(output)],capture_output=True,text=True,encoding='utf-8')
            assert duplicate.returncode!=0 and output.read_bytes()==expected.encode('utf-8')
        print(json.dumps({'result':'PASS','api_requests':count,'committed_revisions':revision,'verified':[
            'independent immutable conversion approval','native quantity bands','half-open dated direct rates',
            'missing/identity/chained conversion rejection','atomic whole-quote currency transition',
            'frozen direction/rational/version/date/rounding export','original-date reversal cancellation',
            'frozen historical reports','overlap rejection and adjacent schedules','restart and idempotent retry',
            'UTF-8 byte-stable and no-overwrite CLI exports']},indent=2))
    finally:
        if process.poll() is None:process.kill();process.wait(timeout=10)
