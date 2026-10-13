#!/usr/bin/env python3
"""Create reviewable synthetic documents via the same public command API as the UI."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/debug/copperline-cli');p.add_argument('--data',default='data/review-demo');p.add_argument('--bookings',action='store_true',help='Include the M8 converted quote booking');a=p.parse_args()
process=subprocess.Popen([str(Path(a.binary).resolve()),'--data',a.data,'api'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True, encoding="utf-8")
def call(method,params=None):
    process.stdin.write(json.dumps({'method':method,'params':params})+'\n');process.stdin.flush();out=json.loads(process.stdout.readline());assert out['ok'],out;return out['result']
revision=call('state')['revision']
if revision:
    print('Existing workspace preserved at revision',revision)
else:
    def run(command,params=None,role='analyst',name='Morgan / Pricing'):
        global revision
        request={'request_id':f'demo-{revision+1}','expected_revision':revision,'actor':{'name':name,'role':role},'command':command}
        if params is not None:request['params']=params
        revision=call('execute',request)['revision']
    run('seed_demo',role='admin',name='Demo administrator')
    price={'product_id':'VLV-100','customer_id':'C-ATLAS','date':'2026-10-13','quantity':10000,'discount':1000,'rebate':500}
    run('quote_create',{'id':'Q-1001','title':'October service program','customer_id':'C-ATLAS','date':'2026-10-13'},'sales','Alex / Sales')
    run('quote_add_line',{'id':'Q-1001','version':1,'request':price},'sales','Alex / Sales')
    run('quote_submit',{'id':'Q-1001','version':2},'sales','Alex / Sales')
    run('grid_calculate',{'id':'G-VLV-ATLAS','version':0,'request':price})
    run('grid_submit',{'id':'G-VLV-ATLAS','version':1})
    run('grid_decide',{'id':'G-VLV-ATLAS','version':2,'approve':True,'note':'Synthetic item price reviewed'},'approver','Riley / Approval')
    run('grid_calculate',{'id':'G-VLV-ATLAS','version':3,'request':dict(price,discount=1500)})
    terms={'id':'A-2026','name':'Atlas annual volume rebate','customer_id':'C-ATLAS','currency':'USD','start':'2026-01-01','end':'2027-01-01','tiers':[{'threshold':0,'rate':200},{'threshold':100000,'rate':500}]}
    run('agreement_create',{'terms':terms});run('agreement_submit',{'id':'A-2026','version':1});run('agreement_decide',{'id':'A-2026','version':2,'approve':True,'note':'Retrospective tiers reviewed'},'approver','Riley / Approval')
    run('transactions_import',{'csv':Path('fixtures/transactions.csv').read_text(encoding="utf-8")});run('rebate_accrue',{'id':'R-1001','agreement_id':'A-2026','version':3})
    run('list_calculate',{'id':'PL-OCT','name':'October OEM list','customer_id':'C-ATLAS','date':'2026-10-13','quantity':1000,'currency':'USD'})
    run('list_submit',{'id':'PL-OCT','version':1});run('list_decide',{'id':'PL-OCT','version':2,'approve':True,'note':'Synthetic list reviewed'},'approver','Riley / Approval');run('list_publish',{'id':'PL-OCT','version':3})
    run('quote_decide',{'id':'Q-1001','version':3,'approve':True,'note':'Baseline synthetic quote reviewed'},'approver','Riley / Approval')
    run('quote_approval_policy_set',{'expected_version':0,'thresholds':{'max_discount':1500,'min_margin':2500,'usd_value':100000,'eur_value':100000,'gbp_value':100000}},'admin','Casey / Admin')
    active=call('state')['grid']['G-VLV-ATLAS']['last_approved']
    sourced=dict(price,source={'kind':'grid','id':'G-VLV-ATLAS','approval_revision':active['decision']['revision']})
    run('quote_create',{'id':'Q-2001','title':'Approved source · Finance review','customer_id':'C-ATLAS','date':'2026-10-13'},'sales','Alex / Sales')
    run('quote_add_line',{'id':'Q-2001','version':1,'request':sourced},'sales','Alex / Sales')
    run('quote_submit',{'id':'Q-2001','version':2},'sales','Alex / Sales')
    run('quote_decide',{'id':'Q-2001','version':3,'approve':True,'note':'Commercial review completed; Finance remains required'},'approver','Riley / Approval')
    run('settlement_create',{'id':'ST-1001','accrual_id':'R-1001','reason':'Synthetic initial rebate recognition'})
    run('settlement_submit',{'id':'ST-1001','version':1})
    run('settlement_decide',{'id':'ST-1001','version':2,'approve':True,'note':'Independent initial recognition'},'finance','Jordan / Finance')
    run('correction_create',{'proposal':{'id':'CR-1001','original_transaction_id':'INV-SYN-002','effective_date':'2026-10-15','reason':'Synthetic duplicate invoice reversal','replacement':None}})
    run('correction_submit',{'id':'CR-1001','version':1})
    run('correction_decide',{'id':'CR-1001','version':2,'approve':True,'note':'Independent full reversal review'},'finance','Jordan / Finance')
    run('rebate_accrue',{'id':'R-2001','agreement_id':'A-2026','version':3})
    run('settlement_create',{'id':'ST-2001','accrual_id':'R-2001','reason':'Synthetic tier correction: 50.00 recognized, 18.00 now due'})
    run('settlement_submit',{'id':'ST-2001','version':1})
    contract_rows=[{'id':'RETAIL','product_id':'VLV-100','unit':'each','min_quantity':1000,'max_quantity':10000,'basis':{'kind':'fixed','invoice_unit':9500}},
                   {'id':'BULK','product_id':'VLV-100','unit':'each','min_quantity':10000,'max_quantity':None,'basis':{'kind':'approved_source','request':dict(sourced,discount=0,rebate=0)}}]
    contract_terms={'id':'CP-ATLAS','edition':1,'name':'Atlas service price agreement','customer_id':'C-ATLAS','currency':'USD','start':'2026-10-01','end':'2027-01-01','priority':100,'rows':contract_rows}
    run('contract_create',{'terms':contract_terms})
    run('contract_submit',{'id':'CP-ATLAS','edition':1,'version':1})
    run('contract_decide',{'id':'CP-ATLAS','edition':1,'version':2,'approve':True,'note':'Independent date and volume terms review'},'approver','Riley / Approval')
    contracted=dict(price,date='2026-11-15',discount=0,rebate=0,source={'kind':'customer_contract'})
    run('quote_create',{'id':'Q-3001','title':'November contracted service parts','customer_id':'C-ATLAS','date':'2026-11-15'},'sales','Alex / Sales')
    run('quote_add_line',{'id':'Q-3001','version':1,'request':contracted},'sales','Alex / Sales')
    run('quote_submit',{'id':'Q-3001','version':2},'sales','Alex / Sales')
    run('quote_decide',{'id':'Q-3001','version':3,'approve':True,'note':'Frozen contract source reviewed'},'approver','Riley / Approval')
    successor=dict(contract_terms,edition=2,start='2026-11-01',rows=[dict(contract_rows[0]),dict(contract_rows[1],basis={'kind':'fixed','invoice_unit':8500})])
    run('contract_create',{'terms':successor})
    run('contract_submit',{'id':'CP-ATLAS','edition':2,'version':1})
    run('conversion_import',{'id':'FX-UOM-OCT','name':'Synthetic dated FX & packaging factors','csv':Path('fixtures/conversions.csv').read_text(encoding='utf-8')})
    run('conversion_submit',{'id':'FX-UOM-OCT','version':1})
    run('conversion_decide',{'id':'FX-UOM-OCT','version':2,'approve':True,'note':'Direct USD to EUR; 12 native each per requested box'},'approver','Riley / Approval')
    converted=dict(price,quantity=1000,discount=0,rebate=0,source={'kind':'customer_contract'},conversion={'currency':'EUR','unit':'box'})
    run('quote_create',{'id':'Q-4001','title':'EUR packaging quote · frozen conversions','customer_id':'C-ATLAS','date':'2026-10-13'},'sales','Alex / Sales')
    run('quote_add_line',{'id':'Q-4001','version':1,'request':converted},'sales','Alex / Sales')
    run('quote_submit',{'id':'Q-4001','version':2},'sales','Alex / Sales')
    run('quote_decide',{'id':'Q-4001','version':3,'approve':True,'note':'Native band and dated FX/UOM evidence reviewed'},'approver','Riley / Approval')
    run('report_convert',{'id':'R-EUR-OCT','request':{'start':'2026-10-01','end':'2026-11-01','currency':'EUR'}})
    if a.bookings:
        run('quote_book',{'request':{'id':'BK-EUR-4001','quote_id':'Q-4001','quote_version':4,'invoice_date':'2026-10-20','source':'synthetic-approved-quote'}})
        run('report_convert',{'id':'R-EUR-BOOKED','request':{'start':'2026-10-01','end':'2026-11-01','currency':'EUR'}})
    print('Created synthetic M8' if a.bookings else 'Created synthetic M6','review workspace at revision',revision)
process.stdin.close();assert process.wait(timeout=10)==0
