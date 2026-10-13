#!/usr/bin/env python3
"""Create reviewable synthetic documents via the same public command API as the UI."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/debug/copperline-cli');p.add_argument('--data',default='data/review-demo');a=p.parse_args()
process=subprocess.Popen([str(Path(a.binary).resolve()),'--data',a.data,'api'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
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
    run('transactions_import',{'csv':Path('fixtures/transactions.csv').read_text()});run('rebate_accrue',{'id':'R-1001','agreement_id':'A-2026','version':3})
    run('list_calculate',{'id':'PL-OCT','name':'October OEM list','customer_id':'C-ATLAS','date':'2026-10-13','quantity':1000,'currency':'USD'})
    run('list_submit',{'id':'PL-OCT','version':1});run('list_decide',{'id':'PL-OCT','version':2,'approve':True,'note':'Synthetic list reviewed'},'approver','Riley / Approval');run('list_publish',{'id':'PL-OCT','version':3})
    print('Created synthetic review workspace at revision',revision)
process.stdin.close();assert process.wait(timeout=10)==0
