// One module-owned polling chain per creature. Cancellation invalidates its generation.
void MgLivePatrolPoll(object actor, int generation);

void MgLivePatrolEvent(object actor, int generation, string status)
{
    json e = MgLiveEvent("patrol", actor);
    if(JsonGetType(JsonObjectGet(e,"command_id"))==JSON_TYPE_NULL)
    {
        JsonObjectSetInplace(e,"command_id",JsonString(GetLocalString(actor,"MGLIVE_PATROL_COMMAND")));
        string by=GetLocalString(actor,"MGLIVE_PATROL_BY");if(by!="") e=MgLiveTextField(e,"by",by);
    }
    e = JsonObjectSet(e, "generation", JsonInt(generation));
    e = JsonObjectSet(e, "point", JsonInt(GetLocalInt(actor, "MGLIVE_PATROL_INDEX")));
    e = JsonObjectSet(e, "lap", JsonInt(GetLocalInt(actor, "MGLIVE_PATROL_LAP")));
    e = JsonObjectSet(e, "status", JsonString(status));
    vector p = GetPosition(actor);
    json at = JsonObjectSet(JsonObject(), "x", JsonFloat(p.x));
    at = JsonObjectSet(at, "y", JsonFloat(p.y));
    at = JsonObjectSet(at, "z", JsonFloat(p.z));
    MgLiveQueueEvent(JsonObjectSet(e, "position", at));
}

void MgLivePatrolMove(location dest, int run)
{
    SetCommandable(TRUE);
    ClearAllActions(TRUE);
    ActionMoveToLocation(dest, run);
    SetCommandable(FALSE);
}

// Repath a failed action. Alternating small sideways/backward steps can clear
// the edge of a dynamic blocker; they never teleport or change the patrol goal.
void MgLivePatrolRetry(location dest, int run, int attempt)
{
    SetCommandable(TRUE);
    ClearAllActions(TRUE);
    if (attempt % 2 == 0)
    {
        vector p = GetPosition(OBJECT_SELF);
        vector d = GetPositionFromLocation(dest) - p;
        float length = sqrt(d.x*d.x + d.y*d.y);
        if (length > 0.1)
        {
            float side = 1.5;
            if ((attempt / 2) % 2 == 0) side = -side;
            d = Vector(d.x / length, d.y / length, 0.0);
            vector step = Vector(p.x - d.x*0.5 - d.y*side, p.y - d.y*0.5 + d.x*side, p.z);
            object area = GetArea(OBJECT_SELF);
            if (step.x > 0.5 && step.y > 0.5 && step.x < GetAreaSize(AREA_WIDTH, area)*10-0.5 && step.y < GetAreaSize(AREA_HEIGHT, area)*10-0.5)
                ActionForceMoveToLocation(Location(area, step, 0.0), run, 1.5);
        }
    }
    ActionForceMoveToLocation(dest, run, 8.0);
    SetCommandable(FALSE);
}

void MgLivePatrolIssue(object actor, int generation)
{
    json point = JsonArrayGet(GetLocalJson(actor, "MGLIVE_PATROL_POINTS"), GetLocalInt(actor, "MGLIVE_PATROL_INDEX"));
    vector dest = Vector(JsonGetFloat(JsonObjectGet(point, "x")), JsonGetFloat(JsonObjectGet(point, "y")), JsonGetFloat(JsonObjectGet(point, "z")));
    SetLocalInt(actor, "MGLIVE_PATROL_WAIT", 0);
    SetLocalInt(actor, "MGLIVE_PATROL_RETRIES", 0);
    DeleteLocalInt(actor, "MGLIVE_PATROL_BLOCKED");
    vector p = GetPosition(actor);
    SetLocalFloat(actor, "MGLIVE_PATROL_BEST", sqrt((p.x-dest.x)*(p.x-dest.x)+(p.y-dest.y)*(p.y-dest.y)));
    AssignCommand(actor, MgLivePatrolMove(Location(GetLocalObject(actor, "MGLIVE_PATROL_AREA"), dest, 0.0), GetLocalInt(actor, "MGLIVE_PATROL_RUN")));
    DelayCommand(0.25, MgLivePatrolPoll(actor, generation));
}

void MgLivePatrolPoll(object actor, int generation)
{
    if (!GetIsObjectValid(actor) || GetLocalInt(actor, "MGLIVE_PATROL_ACTIVE") != generation) return;
    if (GetIsDead(actor) || GetArea(actor) != GetLocalObject(actor, "MGLIVE_PATROL_AREA")) { MgLiveCancelHold(actor); return; }
    int index = GetLocalInt(actor, "MGLIVE_PATROL_INDEX");
    int lap = GetLocalInt(actor, "MGLIVE_PATROL_LAP");
    json points = GetLocalJson(actor, "MGLIVE_PATROL_POINTS");
    json point = JsonArrayGet(points, index);
    vector dest = Vector(JsonGetFloat(JsonObjectGet(point, "x")), JsonGetFloat(JsonObjectGet(point, "y")), JsonGetFloat(JsonObjectGet(point, "z")));
    vector position = GetPosition(actor);
    // Walking projects Z onto the walkmesh (e.g. fixture ground is -1.3).
    float dx = position.x - dest.x; float dy = position.y - dest.y;
    float distance = sqrt(dx*dx + dy*dy);
    int reached = distance <= 0.6;
    int wait = GetLocalInt(actor, "MGLIVE_PATROL_WAIT") + 1;
    if (reached || distance < GetLocalFloat(actor, "MGLIVE_PATROL_BEST") - 0.15)
    {
        SetLocalFloat(actor, "MGLIVE_PATROL_BEST", distance);
        wait = 0;
        if (GetLocalInt(actor, "MGLIVE_PATROL_BLOCKED"))
        {
            DeleteLocalInt(actor, "MGLIVE_PATROL_BLOCKED");
            MgLivePatrolEvent(actor, generation, "resumed");
        }
    }
    SetLocalInt(actor, "MGLIVE_PATROL_WAIT", wait);
    if (reached)
    {
        MgLivePatrolEvent(actor, generation, "reached");
        index++;
        if (index >= JsonGetLength(points))
        {
            lap++;
            if (!GetLocalInt(actor, "MGLIVE_PATROL_LOOP")) { MgLiveCancelHold(actor); return; }
            index = 0;
        }
        SetLocalInt(actor, "MGLIVE_PATROL_INDEX", index);
        SetLocalInt(actor, "MGLIVE_PATROL_LAP", lap);
        MgLivePatrolIssue(actor, generation);
    }
    else
    {
        if (wait >= 12)
        {
            if (!GetLocalInt(actor, "MGLIVE_PATROL_BLOCKED"))
            {
                SetLocalInt(actor, "MGLIVE_PATROL_BLOCKED", TRUE);
                MgLivePatrolEvent(actor, generation, "blocked");
            }
            int attempt = GetLocalInt(actor, "MGLIVE_PATROL_RETRIES") + 1;
            // Keep retrying for the lifetime of this order, without integer wrap.
            if (attempt > 1000000) attempt = 1;
            SetLocalInt(actor, "MGLIVE_PATROL_RETRIES", attempt);
            SetLocalInt(actor, "MGLIVE_PATROL_WAIT", 0);
            AssignCommand(actor, MgLivePatrolRetry(Location(GetArea(actor), dest, 0.0), GetLocalInt(actor, "MGLIVE_PATROL_RUN"), attempt));
        }
        DelayCommand(0.25, MgLivePatrolPoll(actor, generation));
    }
}
