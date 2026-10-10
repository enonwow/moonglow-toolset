// mg_test: assertions for `mg test` (Moonglow Toolset). Marker format 1.
//
// A test is a script named test_*, with a main(), that includes this file
// and calls the functions below. `mg test MODULE` runs every one of them in
// the game's server, each as an event of its own, and reads what they wrote
// to the server's log:
//
//     MG_TEST PASS <name>
//     MG_TEST FAIL <name> :: <detail>
//
// A module that runs its tests itself (`mg test --driver none`) calls
// TestsDone() when the last one is through.

const int MG_TEST_FORMAT = 1;

// The name the following assertions are grouped under ("" for none).
void Describe(string sGroup);

// One passed, or failed, test case by name.
void Pass(string sName);
void Fail(string sName, string sDetail = "");

// Passes if bCondition, else fails with sDetail. Returns bCondition.
int Assert(int bCondition, string sName, string sDetail = "");

// Pass if the two are equal; a failure says what was got and expected.
int AssertEqualInt(int nActual, int nExpected, string sName);
int AssertEqualString(string sActual, string sExpected, string sName);
int AssertEqualFloat(float fActual, float fExpected, string sName, float fWithin = 0.0001);

// Passes if oObject is a valid object.
int AssertValid(object oObject, string sName);

// All tests are through: `mg test` stops the server. Only for a module
// that runs its tests itself; `mg test`'s own driver says it.
void TestsDone();

string MgTestName(string sName)
{
    string sGroup = GetLocalString(GetModule(), "MG_TEST_GROUP");
    if (sGroup == "") return sName;
    return sGroup + ": " + sName;
}

void Describe(string sGroup)
{
    SetLocalString(GetModule(), "MG_TEST_GROUP", sGroup);
}

void Pass(string sName)
{
    WriteTimestampedLogEntry("MG_TEST PASS " + MgTestName(sName));
}

void Fail(string sName, string sDetail = "")
{
    if (sDetail == "")
        WriteTimestampedLogEntry("MG_TEST FAIL " + MgTestName(sName));
    else
        WriteTimestampedLogEntry("MG_TEST FAIL " + MgTestName(sName) + " :: " + sDetail);
}

int Assert(int bCondition, string sName, string sDetail = "")
{
    if (bCondition) Pass(sName);
    else Fail(sName, sDetail);
    return bCondition;
}

int AssertEqualInt(int nActual, int nExpected, string sName)
{
    return Assert(nActual == nExpected, sName,
        "got " + IntToString(nActual) + ", expected " + IntToString(nExpected));
}

int AssertEqualString(string sActual, string sExpected, string sName)
{
    return Assert(sActual == sExpected, sName,
        "got \"" + sActual + "\", expected \"" + sExpected + "\"");
}

int AssertEqualFloat(float fActual, float fExpected, string sName, float fWithin = 0.0001)
{
    return Assert(fabs(fActual - fExpected) <= fWithin, sName,
        "got " + FloatToString(fActual, 0, 6) + ", expected " + FloatToString(fExpected, 0, 6));
}

int AssertValid(object oObject, string sName)
{
    return Assert(GetIsObjectValid(oObject), sName, "not a valid object");
}

void TestsDone()
{
    WriteTimestampedLogEntry("MG_TEST DONE");
}
