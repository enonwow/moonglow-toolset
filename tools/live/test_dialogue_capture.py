"""Private H11 runtime proof; preparation uses mg_live::build in private_fixture.rs."""
import time

def run(p):
    actor=p.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
    listener=p.command(op="spawn",kind="creature",r="nw_bartender",x=4,y=5,z=0,f=0)["obj"]
    p.chunk('SetLocalObject(GetModule(),"MGET_CAPTURE_LISTENER",StringToObject("'+listener+'"));')
    observed=[]
    for dialog in ["mget_focus","city_chat","city_gate"]:
        p.chunk('SetLocalString(GetModule(),"MGET_CAPTURE_DLG","'+dialog+'");')
        p.command(op="run_script",script="mget_capstart",target=actor)
        p.until(lambda:len([e for e in p.matching("dialog") if e["dialog"]==dialog])>=2)
        rows=[e for e in p.matching("dialog") if e["dialog"]==dialog]
        assert [e["node_type"] for e in rows]==["entry","reply"],rows
        assert all(e["owner"]==actor and e["node"]==0 for e in rows)
        observed.extend(rows)
        end=time.monotonic()+2
        while time.monotonic()<end:p.poll(.2)
    count=p.native('JsonInt(GetLocalInt(GetModule(),"MGET_OLD_ACTIONS"))')
    param=p.native('JsonString(GetLocalString(GetModule(),"MGET_OLD_PARAM"))')
    assert count==2 and param=="original-param",(count,param)
    return {"events":observed,"original_actions":count,"original_param":param,"limitations":"NPC-only headless conversation auto-follows replies. Real PC choice/client token substitution is not proven. SpeakOneLinerConversation does not execute Actions Taken; ordinary speech hook covers its spoken line."}
