"""Trusted translation configuration proof through the real game Redis callback."""
import json


def run(p):
    def publish(config, boot=None):
        p.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({
            "v": 1, "script_version": 2, "boot": boot or p.state["boot"],
            "prefix": "mglive-events", "commands": [], "received": [], "translation": config}))
        assert p.pub.read() > 0

    def locals_():
        return p.native('JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonInt(GetLocalInt(GetModule(),"MGLIVE_XL"))),JsonString(GetLocalString(GetModule(),"MGLIVE_XL_COMMANDS")))')

    assert locals_()[0] == 0
    first = {"id": "a" * 32, "enabled": 1, "prefixes": "/!#"}
    publish(first)
    p.until(lambda: p.state.get("translation_config") == first["id"])
    assert locals_() == [1, "/!#"]
    for bad, boot in [({"id": "b" * 32, "enabled": 0, "prefixes": "?"}, "wrong-boot"),
                      ({"id": "c" * 32, "enabled": 0, "prefixes": "\n"}, None)]:
        seq = p.state["seq"]
        publish(bad, boot)
        p.until(lambda: p.state["seq"] > seq)
        assert p.state.get("translation_config") == first["id"]
        assert locals_() == [1, "/!#"]
    final = {"id": "d" * 32, "enabled": 0, "prefixes": "$"}
    publish(final)
    p.until(lambda: p.state.get("translation_config") == final["id"])
    assert locals_() == [0, "$"]
    return {"updated_without_restart": True, "report_ack": True,
            "wrong_boot_and_invalid_prefix_rejected": True, "default_off": True}
