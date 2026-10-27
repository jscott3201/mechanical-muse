//! Drafts that passed every scenario, keyed by content id. Kept in memory and, when a directory
//! is configured, one JSON file each so they survive a restart. A stored draft is a claim, not a
//! licence: a run replays its vectors again before loading it (AGENTS.md hard rule 5).

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

    pub fn insert(&self, draft: Draft) {
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
        lock(&self.drafts).insert(draft.info.content_id.clone(), draft);
    }

    pub fn get(&self, content_id: &str) -> Option<Draft> {
        lock(&self.drafts).get(content_id).cloned()
    }

    /// Every stored draft, ordered by content id.
    pub fn list(&self) -> Vec<DraftInfo> {
        lock(&self.drafts)
            .values()
            .map(|d| d.info.clone())
            .collect()
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
            Ok(Ok(draft)) => out.push(draft),
            Ok(Err(e)) => {
                tracing::warn!(file = %path.display(), error = %e, "skipping a bad draft")
            }
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "skipping a draft"),
        }
    }
    out
}
