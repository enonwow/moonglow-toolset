"""H6 native readbacks on the owned private server; never launches the demo."""
def run(p):
    results=[]
    for args, expected in [
        ({"op":"set_weather","weather":"rain"},{"weather":1}),
        ({"op":"set_weather","weather":"snow","all":True},{"weather":2}),
        ({"op":"set_weather","weather":"clear"},{"weather":0}),
        ({"op":"set_fog","period":"both","color":0x123456,"amount":42},{"fog_day_color":0x123456,"fog_night_color":0x123456,"fog_day_amount":42,"fog_night_amount":42}),
        ({"op":"set_area_light","period":"day","ambient":0x112233,"diffuse":0x445566},{"light_day_ambient":0x112233,"light_day_diffuse":0x445566}),
        ({"op":"set_area_light","period":"night","ambient":0x776655,"diffuse":0x332211},{"light_night_ambient":0x776655,"light_night_diffuse":0x332211}),
        ({"op":"set_skybox","row":1},{"skybox":1}),
        ({"op":"set_music","slot":"day","track":1},{"music_day":1}),
        ({"op":"set_music","slot":"night","track":2},{"music_night":2}),
        ({"op":"set_music","slot":"battle","track":3,"action":"play"},{"music_battle":3,"battle_playing":True}),
        ({"op":"set_music","slot":"battle","action":"stop"},{"battle_playing":False}),
        ({"op":"set_music","slot":"day","action":"play"},{"music_playing":True}),
        ({"op":"set_music","slot":"day","action":"stop"},{"music_playing":False}),
        ({"op":"set_ambient_sound","period":"day","track":1,"volume":90,"action":"play"},{"ambient_day":1,"ambient_day_volume":90}),
        ({"op":"set_ambient_sound","period":"night","track":2,"volume":20,"action":"stop"},{"ambient_night":2,"ambient_night_volume":20}),
    ]:
        result=p.command(**args)
        for key,value in expected.items(): assert result["value"][key]==value,(args,key,result["value"])
        results.append({"command":args,"readback":result["value"]})
    for args in [
        {"op":"set_weather","weather":"hail"},
        {"op":"set_weather","weather":"rain","all":1},
        {"op":"set_fog","period":"day","amount":101},
        {"op":"set_fog","period":"both"},
        {"op":"set_area_light","period":"night","ambient":16777216},
        {"op":"set_skybox","row":65535},
        {"op":"set_music","slot":"day","track":65535},
        {"op":"set_ambient_sound","period":"day","volume":128},
        {"op":"sound_at","resref":"as_wind1","x":-1,"y":0,"z":0},
    ]: p.command(expect="invalid",**args)
    # Read an installed ambient sound row instead of assuming a resource name.
    sound=p.native('JsonString(Get2DAString("ambientsound","Resource",1))')
    if not sound or sound=="****":
        sound=p.native('JsonString(Get2DAString("ambientsound","Resource1",1))')
    result=p.command(op="sound_at",resref=sound,x=5,y=5,z=0)
    assert result["value"]["submitted"],result
    helper=result["obj"]
    p.until(lambda:not p.native('JsonBool(GetIsObjectValid(StringToObject("'+helper+'")))'),timeout=10)
    return {"native_readbacks":results,"sound":{"resource":sound,"result":result,"helper_removed":True},"limitations":"No connected client: visual fades, weather rendering and sound audibility are not proven."}
