void main()
{
    object m=GetModule();if(!GetLocalInt(m,"ST_RUNNING")) return;
    int now=GetMicrosecondCounter();int before=GetLocalInt(m,"ST_FRAME_AT");SetLocalInt(m,"ST_FRAME_AT",now);
    if(before && now>before) WriteTimestampedLogEntry("ST_FRAME us="+IntToString(now-before));
    DelayCommand(0.05,ExecuteScript("st_frame",m));
}
