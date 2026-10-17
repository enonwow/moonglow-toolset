// JsonToObject needs instance geometry for triggers/encounters; UTT/UTE
// blueprints usually omit it. Supply a small default footprint before loading.
json MgLiveGeometryPoint(float x,float y,float z)
{
    json point=JsonObjectSet(JsonObject(),"__struct_id",JsonInt(3));
    JsonObjectSetInplace(point,"PointX",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("float")),"value",JsonFloat(x)));
    JsonObjectSetInplace(point,"PointY",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("float")),"value",JsonFloat(y)));
    JsonObjectSetInplace(point,"PointZ",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("float")),"value",JsonFloat(z)));
    return point;
}
json MgLiveWithGeometry(json source,json points)
{
    return JsonObjectSet(source,"Geometry",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("list")),"value",points));
}
json MgLiveSpawnFootprint(json source)
{
    if(JsonGetLength(JsonObjectGet(JsonObjectGet(source,"Geometry"),"value"))>=3) return source;
    json points=JsonArray();
    JsonArrayInsertInplace(points,MgLiveGeometryPoint(-1.0,-1.0,0.0));
    JsonArrayInsertInplace(points,MgLiveGeometryPoint(1.0,-1.0,0.0));
    JsonArrayInsertInplace(points,MgLiveGeometryPoint(1.0,1.0,0.0));
    JsonArrayInsertInplace(points,MgLiveGeometryPoint(-1.0,1.0,0.0));
    return MgLiveWithGeometry(source,points);
}

json MgLiveSpawnTrigger(json j,object area)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    if(!MgLivePosition(j) || !MgLiveNumber(JsonObjectGet(j,"f"))) return MgLiveResult(id,"finite xyz/f required");
    string shape=JsonGetString(JsonObjectGet(j,"shape"));
    json radius=JsonObjectGet(j,"radius");json width=JsonObjectGet(j,"width");json height=JsonObjectGet(j,"height");
    float w;float h;int vertices;
    if(shape=="circle")
    {
        if(!MgLiveNumber(radius) || JsonGetType(width)!=JSON_TYPE_NULL || JsonGetType(height)!=JSON_TYPE_NULL) return MgLiveResult(id,"circle requires radius only");
        w=JsonGetFloat(radius);h=w;vertices=16;
    }
    else if(shape=="rectangle")
    {
        if(!MgLiveNumber(width) || !MgLiveNumber(height) || JsonGetType(radius)!=JSON_TYPE_NULL) return MgLiveResult(id,"rectangle requires width/height only");
        w=JsonGetFloat(width);h=JsonGetFloat(height);vertices=4;
    }
    else return MgLiveResult(id,"shape must be circle or rectangle");
    if(w<=0.0 || h<=0.0 || w>30.0 || h>30.0) return MgLiveResult(id,"dimensions must be greater than 0 and at most 30 metres");
    vector center=Vector(JsonGetFloat(JsonObjectGet(j,"x")),JsonGetFloat(JsonObjectGet(j,"y")),JsonGetFloat(JsonObjectGet(j,"z")));
    vector axis=AngleToVector(JsonGetFloat(JsonObjectGet(j,"f")));
    string geometry="";json points=JsonArray();int i;
    for(i=0;i<vertices;i++)
    {
        float x;float y;
        if(vertices==16) { vector dir=AngleToVector(IntToFloat(i)*22.5);x=dir.x*w;y=dir.y*w; }
        else { x=w*0.5;y=h*0.5;if(i==0 || i==3) x=-x;if(i<2) y=-y; }
        vector relative=Vector(x*axis.x-y*axis.y,x*axis.y+y*axis.x,0.0);
        JsonArrayInsertInplace(points,MgLiveGeometryPoint(relative.x,relative.y,relative.z));
        vector at=center+relative;
        if(at.x<0.0 || at.y<0.0 || at.x>=IntToFloat(GetAreaSize(AREA_WIDTH,area)*10) || at.y>=IntToFloat(GetAreaSize(AREA_HEIGHT,area)*10)) return MgLiveResult(id,"trigger vertices outside area");
        geometry+="{"+FloatToString(at.x,0,5)+","+FloatToString(at.y,0,5)+","+FloatToString(at.z,0,5)+"}";
    }
    json source=JsonObjectSet(JsonObject(),"__data_type",JsonString("UTT "));
    json requested=JsonObjectGet(j,"r");
    if(JsonGetType(requested)!=JSON_TYPE_NULL)
    {
        if(JsonGetType(requested)!=JSON_TYPE_STRING || !MgLiveResRef(JsonGetString(requested))) return MgLiveResult(id,"invalid UTT resref");
        if(ResManGetAliasFor(JsonGetString(requested),RESTYPE_UTT)=="") return MgLiveResult(id,"UTT not found",OBJECT_INVALID,"not_found");
        source=TemplateToJson(JsonGetString(requested),RESTYPE_UTT);
    }
    source=MgLiveWithGeometry(source,points);
    object made=JsonToObject(source,Location(area,center,JsonGetFloat(JsonObjectGet(j,"f"))));
    if(!GetIsObjectValid(made)) return MgLiveResult(id,"generic trigger creation failed");
    if(JsonGetType(requested)==JSON_TYPE_NULL) SetTag(made,"MGLIVE_TRIGGER");
    SetLocalInt(made,"MGLIVE_OWN",TRUE);
    NWNX_Object_SetTriggerGeometry(made,geometry);
    json value=JsonObjectSet(JsonObject(),"geometry",JsonString(NWNX_Object_GetTriggerGeometry(made)));
    JsonObjectSetInplace(value,"contains_center",JsonBool(NWNX_Object_GetPositionIsInTrigger(made,center)));
    if(!NWNX_Object_GetPositionIsInTrigger(made,center))
    {
        DestroyObject(made);
        return MgLiveResult(id,"trigger geometry readback failed; object removed");
    }
    return JsonObjectSet(MgLiveResult(id,"",made),"value",value);
}
