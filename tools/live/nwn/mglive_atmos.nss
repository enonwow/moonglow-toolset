#include "nwnx_area"

// Native readback, not a cache of requested values. Sound playback itself has
// no native getter; music does. Fog/light fades are submitted separately.
json MgLiveAtmosphereState(object a)
{
    json v=JsonObject();
    JsonObjectSetInplace(v,"weather",JsonInt(GetWeather(a)));
    JsonObjectSetInplace(v,"skybox",JsonInt(GetSkyBox(a)));
    JsonObjectSetInplace(v,"fog_day_color",JsonInt(GetFogColor(FOG_TYPE_SUN,a)));
    JsonObjectSetInplace(v,"fog_night_color",JsonInt(GetFogColor(FOG_TYPE_MOON,a)));
    JsonObjectSetInplace(v,"fog_day_amount",JsonInt(GetFogAmount(FOG_TYPE_SUN,a)));
    JsonObjectSetInplace(v,"fog_night_amount",JsonInt(GetFogAmount(FOG_TYPE_MOON,a)));
    JsonObjectSetInplace(v,"light_day_ambient",JsonInt(GetAreaLightColor(AREA_LIGHT_COLOR_SUN_AMBIENT,a)));
    JsonObjectSetInplace(v,"light_day_diffuse",JsonInt(GetAreaLightColor(AREA_LIGHT_COLOR_SUN_DIFFUSE,a)));
    JsonObjectSetInplace(v,"light_night_ambient",JsonInt(GetAreaLightColor(AREA_LIGHT_COLOR_MOON_AMBIENT,a)));
    JsonObjectSetInplace(v,"light_night_diffuse",JsonInt(GetAreaLightColor(AREA_LIGHT_COLOR_MOON_DIFFUSE,a)));
    JsonObjectSetInplace(v,"music_day",JsonInt(MusicBackgroundGetDayTrack(a)));
    JsonObjectSetInplace(v,"music_night",JsonInt(MusicBackgroundGetNightTrack(a)));
    JsonObjectSetInplace(v,"music_battle",JsonInt(MusicBackgroundGetBattleTrack(a)));
    JsonObjectSetInplace(v,"music_playing",JsonBool(NWNX_Area_GetMusicIsPlaying(a)));
    JsonObjectSetInplace(v,"battle_playing",JsonBool(NWNX_Area_GetMusicIsPlaying(a,TRUE)));
    JsonObjectSetInplace(v,"ambient_day",JsonInt(NWNX_Area_GetAmbientSoundDay(a)));
    JsonObjectSetInplace(v,"ambient_night",JsonInt(NWNX_Area_GetAmbientSoundNight(a)));
    JsonObjectSetInplace(v,"ambient_day_volume",JsonInt(NWNX_Area_GetAmbientSoundDayVolume(a)));
    JsonObjectSetInplace(v,"ambient_night_volume",JsonInt(NWNX_Area_GetAmbientSoundNightVolume(a)));
    return v;
}

json MgLiveAtmosphere(json j, object a)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    string op=JsonGetString(JsonObjectGet(j,"op"));
    string period=JsonGetString(JsonObjectGet(j,"period"));
    json color=JsonObjectGet(j,"color");json amount=JsonObjectGet(j,"amount");
    json fade=JsonObjectGet(j,"fade");float seconds=0.0;
    if(JsonGetType(fade)!=JSON_TYPE_NULL)
    {
        if(!MgLiveNumber(fade) || JsonGetFloat(fade)<0.0 || JsonGetFloat(fade)>30.0) return MgLiveResult(id,"fade must be 0-30 seconds");
        seconds=JsonGetFloat(fade);
    }
    if(op=="set_weather")
    {
        string weather=JsonGetString(JsonObjectGet(j,"weather"));int w=-2;
        if(weather=="clear") w=WEATHER_CLEAR;else if(weather=="rain") w=WEATHER_RAIN;
        else if(weather=="snow") w=WEATHER_SNOW;else if(weather=="default") w=WEATHER_USE_AREA_SETTINGS;
        if(w==-2) return MgLiveResult(id,"invalid weather");
        json all=JsonObjectGet(j,"all");
        if(JsonGetType(all)!=JSON_TYPE_NULL && JsonGetType(all)!=JSON_TYPE_BOOL) return MgLiveResult(id,"all must be boolean");
        object dest=a;if(JsonGetInt(all)) dest=GetModule();
        else if(GetIsAreaInterior(a)) return MgLiveResult(id,"weather requires an outdoor area");
        SetWeather(dest,w);
    }
    else if(op=="set_skybox")
    {
        json row=JsonObjectGet(j,"row");
        if(!MgLiveInt(row,0,65535) || JsonGetInt(row)>=Get2DARowCount("skyboxes")) return MgLiveResult(id,"invalid skyboxes.2da row");
        SetSkyBox(JsonGetInt(row),a);
    }
    else if(op=="set_fog")
    {
        int type=-1;if(period=="day") type=FOG_TYPE_SUN;else if(period=="night") type=FOG_TYPE_MOON;else if(period=="both") type=FOG_TYPE_ALL;
        if(type<0 || (JsonGetType(color)==JSON_TYPE_NULL && JsonGetType(amount)==JSON_TYPE_NULL)) return MgLiveResult(id,"period and color or amount required");
        if(JsonGetType(color)!=JSON_TYPE_NULL && !MgLiveInt(color,0,16777215)) return MgLiveResult(id,"color must be RGB integer 0-16777215");
        if(JsonGetType(amount)!=JSON_TYPE_NULL && !MgLiveInt(amount,0,100)) return MgLiveResult(id,"amount must be 0-100");
        if(JsonGetType(color)!=JSON_TYPE_NULL) SetFogColor(type,JsonGetInt(color),a,seconds);
        if(JsonGetType(amount)!=JSON_TYPE_NULL) SetFogAmount(type,JsonGetInt(amount),a);
    }
    else if(op=="set_area_light")
    {
        json ambient=JsonObjectGet(j,"ambient");json diffuse=JsonObjectGet(j,"diffuse");
        if((period!="day" && period!="night") || (JsonGetType(ambient)==JSON_TYPE_NULL && JsonGetType(diffuse)==JSON_TYPE_NULL)) return MgLiveResult(id,"period and ambient or diffuse required");
        if((JsonGetType(ambient)!=JSON_TYPE_NULL && !MgLiveInt(ambient,0,16777215)) || (JsonGetType(diffuse)!=JSON_TYPE_NULL && !MgLiveInt(diffuse,0,16777215))) return MgLiveResult(id,"light colors must be RGB integers 0-16777215");
        int type=AREA_LIGHT_COLOR_SUN_AMBIENT;if(period=="night") type=AREA_LIGHT_COLOR_MOON_AMBIENT;
        if(JsonGetType(ambient)!=JSON_TYPE_NULL) SetAreaLightColor(type,JsonGetInt(ambient),a,seconds);
        if(JsonGetType(diffuse)!=JSON_TYPE_NULL) SetAreaLightColor(type+1,JsonGetInt(diffuse),a,seconds);
    }
    else if(op=="set_music" || op=="set_ambient_sound")
    {
        json track=JsonObjectGet(j,"track");json actionValue=JsonObjectGet(j,"action");string audioAction=JsonGetString(actionValue);
        json volume=JsonObjectGet(j,"volume");int music=op=="set_music";
        string slot=period;if(music) slot=JsonGetString(JsonObjectGet(j,"slot"));
        if(slot!="day" && slot!="night" && !(music && slot=="battle")) return MgLiveResult(id,"invalid audio slot");
        if(JsonGetType(track)==JSON_TYPE_NULL && JsonGetType(actionValue)==JSON_TYPE_NULL && (music || JsonGetType(volume)==JSON_TYPE_NULL)) return MgLiveResult(id,"audio change required");
        if(JsonGetType(actionValue)!=JSON_TYPE_NULL && audioAction!="play" && audioAction!="stop") return MgLiveResult(id,"action must be play or stop");
        string table="ambientsound";if(music) table="ambientmusic";
        if(JsonGetType(track)!=JSON_TYPE_NULL && (!MgLiveInt(track,0,65535) || JsonGetInt(track)>=Get2DARowCount(table))) return MgLiveResult(id,"audio row absent from loaded 2da");
        if(JsonGetType(volume)!=JSON_TYPE_NULL && !MgLiveInt(volume,0,127)) return MgLiveResult(id,"volume must be 0-127");
        if(music)
        {
            if(JsonGetType(track)!=JSON_TYPE_NULL)
            {
                if(slot=="day") MusicBackgroundChangeDay(a,JsonGetInt(track));else if(slot=="night") MusicBackgroundChangeNight(a,JsonGetInt(track));else MusicBattleChange(a,JsonGetInt(track));
            }
            if(audioAction=="play") { if(slot=="battle") MusicBattlePlay(a);else MusicBackgroundPlay(a); }
            if(audioAction=="stop") { if(slot=="battle") MusicBattleStop(a);else MusicBackgroundStop(a); }
        }
        else
        {
            if(JsonGetType(track)!=JSON_TYPE_NULL) { if(slot=="day") AmbientSoundChangeDay(a,JsonGetInt(track));else AmbientSoundChangeNight(a,JsonGetInt(track)); }
            if(JsonGetType(volume)!=JSON_TYPE_NULL) { if(slot=="day") AmbientSoundSetDayVolume(a,JsonGetInt(volume));else AmbientSoundSetNightVolume(a,JsonGetInt(volume)); }
            if(audioAction=="play") AmbientSoundPlay(a);else if(audioAction=="stop") AmbientSoundStop(a);
        }
    }
    else if(op=="sound_at")
    {
        string resref=JsonGetString(JsonObjectGet(j,"resref"));
        if(!MgLiveResRef(resref) || !MgLivePosition(j)) return MgLiveResult(id,"resref and finite xyz required");
        float x=JsonGetFloat(JsonObjectGet(j,"x"));float y=JsonGetFloat(JsonObjectGet(j,"y"));
        if(x<0.0 || y<0.0 || x>=IntToFloat(GetAreaSize(AREA_WIDTH,a)*10) || y>=IntToFloat(GetAreaSize(AREA_HEIGHT,a)*10)) return MgLiveResult(id,"point outside area");
        json source=JsonObjectSet(JsonObject(),"__data_type",JsonString("UTW "));
        object helper=JsonToObject(source,Location(a,Vector(x,y,JsonGetFloat(JsonObjectGet(j,"z"))),0.0));
        if(!GetIsObjectValid(helper)) return MgLiveResult(id,"sound helper creation failed");
        SetLocalInt(helper,"MGLIVE_SOUND_HELPER",TRUE);

        SetLocalString(helper,"MGLIVE_SOUND",resref);
        ExecuteScript("mglive_sound",helper);
        DestroyObject(helper,2.0);
        return JsonObjectSet(MgLiveResult(id,"",helper),"value",JsonObjectSet(JsonObject(),"submitted",JsonBool(GetLocalInt(helper,"MGLIVE_SOUND_SUBMITTED"))));
    }
    return JsonObjectSet(MgLiveResult(id),"value",MgLiveAtmosphereState(a));
}
