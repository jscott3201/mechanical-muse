//! Loading CXF with a receipt. A load succeeds only when the receipt is empty apart from
//! allowlisted warnings, and the export has a complete content id.

use oce_api::Engine;
use serde::Serialize;

use crate::dto::{DiagnosticDto, EngineErrorDto, stage_name};

/// Warning codes that do not fail a draft. Empty until a decision lists some
/// (`docs/DECISIONS.md`, pending).
pub const ALLOWED_LOAD_WARNINGS: &[&str] = &[];

/// A model the engine accepted, with the identity verification receipts refer to.
pub struct Loaded {
    pub engine: Engine,
    pub content_id: String,
}

/// Why a load was refused. `code` is always set: the first diagnostic's code, or one of ours
/// (`load-failed-<stage>`, `warning-not-allowed`, `export-incomplete`) when the engine gave none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("CXF load failed at {stage}: {code}")]
pub struct LoadFailure {
    pub stage: &'static str,
    pub code: String,
    pub diagnostics: Vec<DiagnosticDto>,
    pub error: Option<EngineErrorDto>,
}

/// Loads CXF into a fresh engine.
pub fn load_cxf(bytes: &[u8]) -> Result<Loaded, LoadFailure> {
    let mut engine = Engine::in_memory();
    let receipt = match engine.load_cxf_with_receipt(bytes) {
        Ok(receipt) => receipt,
        Err(failure) => {
            let diagnostics: Vec<_> = failure
                .diagnostics()
                .records()
                .iter()
                .map(DiagnosticDto::from_record)
                .collect();
            let stage = stage_name(failure.stage());
            return Err(LoadFailure {
                stage,
                code: diagnostics
                    .first()
                    .map_or_else(|| format!("load-failed-{stage}"), |d| d.code.clone()),
                diagnostics,
                error: Some(EngineErrorDto::from_error(failure.error())),
            });
        }
    };
    let diagnostics: Vec<_> = receipt
        .diagnostics()
        .records()
        .iter()
        .map(DiagnosticDto::from_record)
        .collect();
    if let Some(blocking) = diagnostics
        .iter()
        .find(|d| !ALLOWED_LOAD_WARNINGS.contains(&d.code.as_str()))
    {
        return Err(LoadFailure {
            stage: blocking.stage,
            code: "warning-not-allowed".into(),
            diagnostics,
            error: None,
        });
    }
    let content_id = engine
        .export_cxf()
        .ok()
        .and_then(|report| report.content_id_complete().ok())
        .ok_or_else(|| LoadFailure {
            stage: "export",
            code: "export-incomplete".into(),
            diagnostics: Vec::new(),
            error: None,
        })?;
    Ok(Loaded { engine, content_id })
}
