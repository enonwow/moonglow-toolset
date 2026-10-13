#!/usr/bin/env python3
"""Find each form's StrRef table in the exe's .data section, by voting.

A form's table is an array of 12-byte records {caption, hint, help StrRef},
indexed by a control's Tag. For every control with a Tag and a caption, each
place in .data that holds a StrRef whose talk-table text is that caption
votes for the table base it would imply (place - Tag * 12). The base with
the most votes is the form's; `gen.py` takes it when the vote is clear, and
`mapping.py`'s LOC_OVERRIDE settles the others (checked with loccheck.py).

    python3 tools/aurora/uiinv/matchloc.py     # rewrites locbases.json, prints the votes
"""
import collections
import json
import re
import struct


def norm(s): return re.sub(r'[^a-z0-9]','',s.replace('&','').lower())


def vote(forms, T, b, say=None):
    idx=collections.defaultdict(set)
    for k,v in T.items():
        n=norm(v)
        if n and len(v)<200: idx[n].add(k)
    # int32 occurrences
    occ=collections.defaultdict(list)
    for o in range(0,len(b)-3,4):
        v=struct.unpack_from('<i',b,o)[0]
        if 0<v<130000: occ[v].append(o)
    def walk(c,acc):
        p=c['props']
        t=p.get('Tag')
        cap=p.get('Caption')
        if cap is None: cap=p.get('Hint')
        if isinstance(t,int) and t>=2 and isinstance(cap,str) and norm(cap):
            acc.append((t,cap,c['name']))
        for k in c['children']: walk(k,acc)
    res={}
    for fn,f in forms.items():
        acc=[]; walk(f,acc)
        votes=collections.Counter()
        for t,cap,nm in acc:
            for s in idx.get(norm(cap),()):
                for o in occ.get(s,()):
                    base=o-t*12
                    if base>=0: votes[base]+=1
        if not votes:
            res[fn]=None
            if say: say(f'{fn} no votes {len(acc)}')
            continue
        (base,v),*rest=votes.most_common(2)+[(None,0)]
        second=rest[0][1] if rest else 0
        res[fn]={'base':base,'votes':v,'n':len(acc),'second':second}
        if say: say(f'{fn:32s} base={base:8d} votes={v:3d}/{len(acc):3d} second={second}')
    return res


if __name__=='__main__':
    import common
    res=vote(common.forms(),common.tlk(),common.exe_data(),print)
    json.dump(res,open(common.work_dir()/'locbases.json','w'),indent=1)
