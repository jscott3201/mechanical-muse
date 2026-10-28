//! Drafts that passed every scenario, keyed by content id. Kept in memory and, when a directory
//! is configured, one JSON file each so they survive a restart. A stored draft is a claim, not a
//! licence: a run checks and replays it again before loading it (AGENTS.md hard rule 5). The
//! labels in `info` (model, source, attempts) are not verified on load; only the graph is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mm_graph::BlockGraph;
use serde::{Deserialize, Serialize};

use crate::author_dto::DraftInfo;
use crate::journal::lock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub info: DraftInfo,
    pub graph: BlockGraph,
}

#[derive(Debug, Default)]
pub struct DraftStore {
    drafts: Mutex<BTreeMap<String, Draft>>,
    dir: Option<PathBuf>,
}

/// A file name for a content id (which may contain `:`), kept to a portable character set.
fn file_name(content_id: &str) -> String {
    let safe: String = content_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{safe}.json")
}

impl DraftStore {
    /// Loads every readable draft under `dir`; unreadable files are logged and skipped.
    pub fn open(dir: Option<PathBuf>) -> Self {
        let mut drafts = BTreeMap::new();
        if let Some(dir) = &dir {
            for draft in read_dir(dir) {
                drafts.insert(draft.info.content_id.clone(), draft);
            }
        }
        Self {
            drafts: Mutex::new(drafts),
            dir,
        }
    }

    /// Stores `draft` as the newest one (a draft verified again moves to the front) and
    /// returns its info with the store order it was given.
    pub fn insert(&self, mut draft: Draft) -> DraftInfo {
        let mut drafts = lock(&self.drafts);
        draft.info.stored_seq = drafts
            .values()
            .map(|d| d.info.stored_seq)
            .max()
            .map_or(1, |n| n.saturating_add(1));
        if let Some(dir) = &self.dir {
            let path = dir.join(file_name(&draft.info.content_id));
            let written = std::fs::create_dir_all(dir).and_then(|()| {
                let json = serde_json::to_vec_pretty(&draft).map_err(std::io::Error::other)?;
                std::fs::write(&path, json)
            });
            if let Err(e) = written {
                tracing::warn!(error = %e, "draft kept in memory only");
            }
        }
        let info = draft.info.clone();
        drafts.insert(info.content_id.clone(), draft);
        info
    }

    pub fn get(&self, content_id: &str) -> Option<Draft> {
        lock(&self.drafts).get(content_id).cloned()
    }

    /// Every stored draft, newest first (drafts from before store order, all 0, last and by
    /// content id).
    pub fn list(&self) -> Vec<DraftInfo> {
        let mut list: Vec<DraftInfo> = lock(&self.drafts)
            .values()
            .map(|d| d.info.clone())
            .collect();
        list.sort_by(|a, b| {
            b.stored_seq
                .cmp(&a.stored_seq)
                .then_with(|| a.content_id.cmp(&b.content_id))
        });
        list
    }
}

fn read_dir(dir: &Path) -> Vec<Draft> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        match std::fs::read(&path).map(|b| serde_json::from_slice::<Draft>(&b)) {
            // One file per content id, named after it: a copy under another name is ignored, so
            // two files never compete for one id.
            Ok(Ok(draft))
                if path.file_name().and_then(|n| n.to_str())
                    == Some(file_name(&draft.info.content_id).as_str()) =>
            {
                out.push(draft);
            }
            Ok(Ok(_)) => {
                tracing::warn!(file = %path.display(), "skipping a draft filed under another name")
            }
            Ok(Err(e)) => {
                tracing::warn!(file = %path.display(), error = %e, "skipping a bad draft")
            }
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "skipping a draft"),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::author_dto::ModelSource;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drafts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn draft(content_id: &str) -> Draft {
        let graph: BlockGraph = serde_json::from_slice(
            &std::fs::read(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../specs/chw_plant_reset/graph.json"),
            )
            .unwrap(),
        )
        .unwrap();
        Draft {
            info: DraftInfo {
                content_id: content_id.into(),
                spec: "chw_plant_reset".into(),
                scenarios: 10,
                model: "muse-spark-1.3".into(),
                source: ModelSource::Recorded,
                session: "test".into(),
                attempts_used: 1,
                stored_seq: 0,
            },
            graph,
        }
    }

    fn ids(store: &DraftStore) -> Vec<String> {
        store.list().into_iter().map(|d| d.content_id).collect()
    }

    /// Content ids are hashes: the list must follow store order, newest first, across a
    /// restart, and a draft verified again becomes the newest.
    #[test]
    fn drafts_list_newest_first_and_keep_their_order_on_disk() {
        let dir = scratch("order");
        let store = DraftStore::open(Some(dir.clone()));
        for id in ["cxf:c", "cxf:a", "cxf:b"] {
            store.insert(draft(id));
        }
        assert_eq!(ids(&store), ["cxf:b", "cxf:a", "cxf:c"]);
        let reopened = DraftStore::open(Some(dir.clone()));
        assert_eq!(ids(&reopened), ["cxf:b", "cxf:a", "cxf:c"]);
        assert_eq!(reopened.insert(draft("cxf:c")).stored_seq, 4);
        assert_eq!(ids(&reopened), ["cxf:c", "cxf:b", "cxf:a"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A draft file written before store order existed still loads, as the oldest.
    #[test]
    fn a_draft_without_store_order_still_loads() {
        let dir = scratch("legacy");
        let mut old = serde_json::to_value(draft("cxf:old")).unwrap();
        old["info"].as_object_mut().unwrap().remove("stored_seq");
        std::fs::write(dir.join(file_name("cxf:old")), old.to_string()).unwrap();
        let store = DraftStore::open(Some(dir.clone()));
        assert_eq!(store.get("cxf:old").unwrap().info.stored_seq, 0);
        store.insert(draft("cxf:new"));
        assert_eq!(ids(&store), ["cxf:new", "cxf:old"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
