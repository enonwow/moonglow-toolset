//! H11's private fixture uses the production Rust instrumenter.
use mg_core::ResType;
use mg_gff::Value;
use mg_module::Module;
use mg_resman::ResKey;

pub(super) fn prepare(module: &mut Module, output: &std::path::Path) {
    let source = std::env::var("MGLIVE_DIALOGUE_SOURCE").expect("read-only dialogue source");
    let source = if cfg!(windows) && source.starts_with("/mnt/") {
        format!("{}:{}", &source[5..6], &source[6..])
    } else {
        source
    };
    let input = Module::open(std::path::Path::new(&source)).unwrap();
    for k in input.keys_of(ResType::DLG) {
        module.set(*k, input.get(k).unwrap().to_vec());
    }
    let key = |n| ResKey::from_filename(n).unwrap();
    let mut focus = module.gff(&key("mget_focus.dlg")).unwrap().unwrap();
    for list in ["EntryList", "ReplyList"] {
        for node in focus.root.list_mut(list).unwrap() {
            node.set("Script", Value::ResRef(b"mget_oldact".to_vec()));
        }
    }
    module.set_gff(key("mget_focus.dlg"), &focus).unwrap();
    module.set(key("mget_oldact.nss"), b"void main(){object m=GetModule();SetLocalInt(m,\"MGET_OLD_ACTIONS\",GetLocalInt(m,\"MGET_OLD_ACTIONS\")+1);SetLocalString(m,\"MGET_OLD_PARAM\",GetScriptParam(\"preserved\"));}\n".to_vec());
    module.set(key("mget_capstart.nss"), b"#include \"nwnx_object\"\nvoid main(){string d=GetLocalString(GetModule(),\"MGET_CAPTURE_DLG\");NWNX_Object_SetDialogResref(OBJECT_SELF,d);ActionStartConversation(GetLocalObject(GetModule(),\"MGET_CAPTURE_LISTENER\"),d,FALSE,FALSE);}\n".to_vec());
    let changes = mg_live::build::capture_dialogue(module).unwrap();
    std::fs::write(output.join("dialogue-inputs.json"), serde_json::to_vec_pretty(&serde_json::json!({"source":source,"nodes":changes,"implementation":"mg_live::build::capture_dialogue"})).unwrap()).unwrap();
}
