#include "mglive_xl"
#include "nwnx_creature"
#include "nwnx_object"

void SendLine(int channel,string text)
{
    object m=GetModule();object speaker=GetLocalObject(m,"MGET_XL_SPEAKER");
    string id=GetStringLeft(text,FindSubString(text," "));
    SetLocalInt(m,"MGET_XL_AT_"+id,GetMicrosecondCounter());
    if(channel==99) AssignCommand(speaker,SpeakString(text,TALKVOLUME_TALK));
    else if(channel==4 || channel==20)
    {
        // A non-client NPC cannot enter the native player-ID tell route.
        // Exercise the identical capture/delivery functions with an explicit target.
        MgXlCapture(channel,text,speaker,GetObjectByTag("XL_NEAR"));
    }
    else NWNX_Chat_SendMessage(channel,text,speaker);
}

void Burst(int n,int count,int delay)
{
    SendLine(1,"xl_b"+IntToString(delay)+"_"+IntToString(n)+" hello *waves*");
    if(n+1<count) DelayCommand(0.05,Burst(n+1,count,delay));
}

void main()
{
    object m=GetModule();int mode=GetLocalInt(m,"MGET_XL_MODE");
    if(mode==1)
    {
        object area=GetObjectByTag("EVENTS");
        if(!GetIsObjectValid(area)) area=GetArea(GetObjectByTag("ST_0_0"));
        json players=JsonArray();int i;
        for(i=0;i<8;i++)
        {
            string tag="XL_SPEAKER";string lang="en";float x=5.0;float y=5.0;
            if(i==1) {tag="XL_SAME";x=6.0;}
            if(i==2) {tag="XL_NEAR";lang="pl";x=7.0;}
            if(i==3) {tag="XL_MID";lang="de";x=12.0;}
            if(i==4) {tag="XL_FAR";lang="fr";x=40.0;}
            if(i==5) {tag="XL_OTHER";lang="pl";x=5.0;}
            if(i==6) {tag="XL_DM";lang="pl";x=6.5;}
            if(i==7) {tag="XL_NPC";y=7.0;}
            object a=area;
            if(i==5) {a=GetObjectByTag("EVENTS2");if(!GetIsObjectValid(a)) a=GetArea(GetObjectByTag("ST_1_0"));}
            object npc=CreateObject(OBJECT_TYPE_CREATURE,"nw_bartender",Location(a,Vector(x,y,0.0),0.0),FALSE,tag);
            int handler;for(handler=5000;handler<=5012;handler++) SetEventScript(npc,handler,"");
            SetLocalString(npc,"MGLIVE_LANG",lang);
            SetLocalInt(npc,"MGET_XL_PLAYER",i!=7);SetLocalInt(npc,"MGET_XL_DM",i==6);
            NWNX_Creature_SetFaction(npc,(i==0 || i==2 || i==5) ? 2 : 3);
            SetListening(npc,TRUE);SetListenPattern(npc,"**",1);
            SetEventScript(npc,EVENT_SCRIPT_CREATURE_ON_DIALOGUE,"mget_xlhear");
            AssignCommand(npc,ClearAllActions(TRUE));SetCommandable(FALSE,npc);
            if(i!=7) JsonArrayInsertInplace(players,JsonString(ObjectToString(npc)));
            if(i==0) SetLocalObject(m,"MGET_XL_SPEAKER",npc);
        }
        SetLocalJson(m,"MGET_XL_CLIENTS",players);
        json diagnostic=JsonObjectSet(JsonObject(),"safe",JsonInt(MgXlSafe("xl_c1 hello *waves*","xl_c1 hello *waves* translated-pl")));
        JsonObjectSetInplace(diagnostic,"command",JsonInt(MgXlCommand("xl_c1 hello *waves* translated-pl")));
        JsonObjectSetInplace(diagnostic,"original_marks",JsonString(MgXlMarkup("xl_c1 hello *waves*")));
        JsonObjectSetInplace(diagnostic,"translated_marks",JsonString(MgXlMarkup("xl_c1 hello *waves* translated-pl")));
        JsonObjectSetInplace(diagnostic,"cr_refused",JsonBool(!MgXlSafe("hello","hello"+GetSubString(NWNX_Util_GetAsciiTableString(),13,1)+"injection")));
        JsonObjectSetInplace(diagnostic,"colour_refused",JsonBool(!MgXlSafe("<caaa>hello</c>","<cbbb>hello</c>")));
        SetLocalJson(m,"MGET_XL_DIAGNOSTIC",diagnostic);return;
    }
    if(mode==2)
    {
        Burst(0,GetLocalInt(m,"MGET_XL_COUNT"),GetLocalInt(m,"MGET_XL_DELAY"));return;
    }
    if(mode==3)
    {
        // An untargeted send really enters the native hook at depth zero.
        SetLocalInt(m,"MGLIVE_XL_DELIVERING",TRUE);
        NWNX_Chat_SendMessage(1,"xl_guard no recursion",GetLocalObject(m,"MGET_XL_SPEAKER"));
        DeleteLocalInt(m,"MGLIVE_XL_DELIVERING");return;
    }
    if(mode==5)
    {
        object pc=GetObjectByTag("XL_NEAR");
        SetLocalLocation(m,"MGET_XL_RESTORE",GetLocation(pc));
        SendLine(1,"xl_move hello *waves*");
        SetLocalString(pc,"MGLIVE_LANG","fr");
        SetCommandable(TRUE,pc);
        AssignCommand(pc,JumpToLocation(GetLocation(GetObjectByTag("XL_OTHER"))));return;
    }
    if(mode==6)
    {
        object pc=GetObjectByTag("XL_NEAR");SetLocalString(pc,"MGLIVE_LANG","pl");
        AssignCommand(pc,JumpToLocation(GetLocalLocation(m,"MGET_XL_RESTORE")));return;
    }
    SendLine(GetLocalInt(m,"MGET_XL_CHANNEL"),GetLocalString(m,"MGET_XL_TEXT"));
}
