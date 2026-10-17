//! Byte-preserving dialogue action instrumentation. mg-gff validates and
//! interprets the nodes; new action tables are appended, leaving every original
//! byte after the header intact (including unknown fields and mixed encodings).
use super::{Manifest, hash, key, valid_resref};
use anyhow::{Context, Result, ensure};
use mg_core::ResType;
use mg_gff::{Gff, Struct};
use mg_module::Module;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub dialogue: String,
    pub kind: String,
    pub node: usize,
    pub original: String,
    pub wrapper: String,
}

pub(super) fn instrument(m: &mut Module, previous: Option<&Manifest>) -> Result<Vec<Change>> {
    let mut changes = Vec::new();
    let dialogs: Vec<_> = m.keys_of(ResType::DLG).copied().collect();
    for k in dialogs {
        let data = m.get(&k).unwrap();
        ensure!(data.len() <= 16 * 1024 * 1024, "Dialogue too large: {k}");
        let gff = Gff::read(data).with_context(|| k.to_string())?;
        let mut raw = Raw::new(data)?;
        let mut sources = std::collections::BTreeMap::new();
        for list in ["EntryList", "ReplyList"] {
            let nodes = gff.root.list(list).unwrap_or_default();
            let ids = raw.children(list)?;
            ensure!(nodes.len() == ids.len(), "Invalid {list} in {k}");
            for (index, (node, id)) in nodes.iter().zip(ids).enumerate() {
                let original = node.resref("Script").map(|r| r.to_string()).unwrap_or_default();
                ensure!(valid_resref(&original, true), "Invalid dialogue action in {k}");
                if original.starts_with("mgd_")
                    && previous.is_some_and(|p| {
                        p.scripts.get(&format!("{original}.nss")).is_some_and(|h| {
                            m.get(&key(&format!("{original}.nss"))).is_some_and(|v| hash(v) == *h)
                        })
                    })
                {
                    continue;
                }
                let tag = if list == "EntryList" {
                    node.string("Speaker").unwrap_or_default()
                } else {
                    &[]
                };
                let params: Vec<_> = node
                    .list("ActionParams")
                    .unwrap_or_default()
                    .iter()
                    .map(|p| p.string("Key").unwrap_or_default())
                    .collect();
                let identity =
                    serde_json::to_vec(&(k.resref.to_string(), &original, tag, &params))?;
                let wrapper = format!("mgd_{}", &hash(&identity)[..12]);
                let mut source = format!(
                    "#include \"mglive_dialog\"\nvoid main()\n{{\n    MgLiveDialogLine(\"{}\", {});\n",
                    k.resref,
                    if tag.is_empty() {
                        "OBJECT_INVALID".into()
                    } else {
                        format!("GetObjectByTag({})", literal(tag)?)
                    }
                );
                if !original.is_empty() {
                    for param in params {
                        let p = literal(param)?;
                        source
                            .push_str(&format!("    SetScriptParam({p}, GetScriptParam({p}));\n"));
                    }
                    source.push_str(&format!("    ExecuteScript(\"{original}\", OBJECT_SELF);\n"));
                }
                source.push_str("}\n");
                let script = format!("{wrapper}.nss");
                if let Some(existing) = m.get(&key(&script)) {
                    ensure!(existing == source.as_bytes(), "Dialogue wrapper name clash: {script}");
                } else {
                    ensure!(
                        !m.contains(&key(&format!("{wrapper}.ncs"))),
                        "Dialogue wrapper name clash: {wrapper}.ncs"
                    );
                }
                sources.insert(script, source);
                raw.action(id, wrapper.as_bytes())?;
                changes.push(Change {
                    dialogue: k.to_string(),
                    kind: list.into(),
                    node: index,
                    original,
                    wrapper,
                });
            }
        }
        if !sources.is_empty() {
            let patched = raw.finish()?;
            // The parsed diff must contain no changes except action resrefs.
            let mut actual = Gff::read(&patched)?;
            restore_actions(&mut actual.root, &gff.root);
            ensure!(actual == gff, "Dialogue instrumentation changed non-action data: {k}");
            m.set(k, patched);
            for (name, source) in sources {
                m.set(key(&name), source.into_bytes());
            }
        }
    }
    Ok(changes)
}

fn restore_actions(to: &mut Struct, from: &Struct) {
    for list in ["EntryList", "ReplyList"] {
        if let Some(nodes) = to.list_mut(list) {
            for (a, b) in nodes.iter_mut().zip(from.list(list).unwrap_or_default()) {
                if let Some(v) = b.get("Script") {
                    a.set("Script", v.clone());
                } else {
                    a.remove("Script");
                }
            }
        }
    }
}

fn literal(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.len() <= 256 && !bytes.contains(&0),
        "Unsupported dialogue speaker/parameter key"
    );
    if bytes.is_empty() {
        return Ok("\"\"".into());
    }
    Ok(bytes
        .iter()
        .map(|b| {
            if (32..=126).contains(b) {
                serde_json::to_string(&char::from(*b).to_string()).unwrap()
            } else {
                format!("GetSubString(NWNX_Util_GetAsciiTableString(), {b}, 1)")
            }
        })
        .collect::<Vec<_>>()
        .join(" + "))
}

fn word(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(data.get(at..at + 4).context("Truncated GFF table")?.try_into()?))
}
fn words(data: &[u8]) -> Result<Vec<[u32; 3]>> {
    data.as_chunks::<12>().0.iter().map(|b| Ok([word(b, 0)?, word(b, 4)?, word(b, 8)?])).collect()
}
struct Raw {
    original: Vec<u8>,
    header: [u32; 12],
    structs: Vec<[u32; 3]>,
    fields: Vec<[u32; 3]>,
    labels: Vec<u8>,
    data: Vec<u8>,
    indices: Vec<u8>,
}
impl Raw {
    fn new(bytes: &[u8]) -> Result<Self> {
        let mut header = [0; 12];
        for (i, h) in header.iter_mut().enumerate() {
            *h = word(bytes, 8 + i * 4)?;
        }
        let block = |i: usize, width: usize| -> Result<&[u8]> {
            let start = header[i] as usize;
            let len = (header[i + 1] as usize).checked_mul(width).context("GFF overflow")?;
            bytes
                .get(start..start.checked_add(len).context("GFF overflow")?)
                .context("Truncated GFF block")
        };
        Ok(Self {
            original: bytes.to_vec(),
            header,
            structs: words(block(0, 12)?)?,
            fields: words(block(2, 12)?)?,
            labels: block(4, 16)?.to_vec(),
            data: block(6, 1)?.to_vec(),
            indices: block(8, 1)?.to_vec(),
        })
    }
    fn fields_of(&self, id: usize) -> Result<Vec<usize>> {
        let s = self.structs.get(id).context("Missing GFF struct")?;
        ensure!(s[2] <= 65536, "Too many dialogue fields");
        match s[2] {
            0 => Ok(Vec::new()),
            1 => Ok(vec![s[1] as usize]),
            n => (0..n)
                .map(|i| Ok(word(&self.indices, s[1] as usize + i as usize * 4)? as usize))
                .collect(),
        }
    }
    fn field(&self, id: usize, name: &str) -> Result<Option<usize>> {
        for i in self.fields_of(id)? {
            let f = self.fields.get(i).context("Missing GFF field")?;
            let at = f[1] as usize * 16;
            let label = self.labels.get(at..at + 16).context("Missing label")?;
            if label.split(|b| *b == 0).next() == Some(name.as_bytes()) {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn children(&self, name: &str) -> Result<Vec<usize>> {
        let Some(i) = self.field(0, name)? else {
            return Ok(Vec::new());
        };
        let f = self.fields[i];
        ensure!(f[0] == 15, "Expected GFF list");
        let at = self.header[10] as usize + f[2] as usize;
        let n = word(&self.original, at)? as usize;
        ensure!(n <= 65536, "Too many dialogue nodes");
        (0..n).map(|j| Ok(word(&self.original, at + 4 + j * 4)? as usize)).collect()
    }
    fn action(&mut self, id: usize, name: &[u8]) -> Result<()> {
        let offset = self.data.len() as u32;
        self.data.push(name.len() as u8);
        self.data.extend_from_slice(name);
        if let Some(i) = self.field(id, "Script")? {
            ensure!(self.fields[i][0] == 11, "Expected Script resref");
            self.fields[i][2] = offset;
        } else {
            let mut indices = self.fields_of(id)?;
            let label = self.labels.len() as u32 / 16;
            self.labels.extend_from_slice(b"Script\0\0\0\0\0\0\0\0\0\0");
            indices.push(self.fields.len());
            self.fields.push([11, label, offset]);
            self.structs[id][1] =
                if indices.len() == 1 { indices[0] as u32 } else { self.indices.len() as u32 };
            self.structs[id][2] = indices.len() as u32;
            if indices.len() > 1 {
                for i in indices {
                    self.indices.extend_from_slice(&(i as u32).to_le_bytes());
                }
            }
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Vec<u8>> {
        let tables = [self.structs, self.fields].map(|rows| {
            rows.into_iter()
                .flat_map(|r| r.into_iter().flat_map(u32::to_le_bytes))
                .collect::<Vec<_>>()
        });
        for (i, (block, width)) in tables
            .into_iter()
            .map(|b| (b, 12))
            .chain([(self.labels, 16), (self.data, 1), (self.indices, 1)])
            .enumerate()
        {
            self.header[i * 2] = u32::try_from(self.original.len())?;
            self.header[i * 2 + 1] = u32::try_from(block.len() / width)?;
            self.original.extend_from_slice(&block);
        }
        for (i, h) in self.header.iter().enumerate() {
            self.original[8 + i * 4..12 + i * 4].copy_from_slice(&h.to_le_bytes());
        }
        Ok(self.original)
    }
}
