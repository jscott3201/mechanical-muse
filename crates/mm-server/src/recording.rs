//! Live sessions' replies, kept so a good session can become the recorded fallback.

use std::path::Path;

use mm_agent::{AuthoringTrace, Recording};

/// Writes `<records>/recordings/<session>.json`: the model's replies in order (graph JSON, or
/// the error a call ended with). No request, key or reasoning is kept. To use one as the
/// recorded fallback, copy it to `fixtures/recordings/<spec>.json`.
pub(crate) fn save(records: Option<&Path>, session: &str, trace: &AuthoringTrace) {
    let Some(dir) = records.map(|d| d.join("recordings")) else {
        return;
    };
    let mut recording = Recording::from_trace(trace);
    recording.note = format!("mm-server live session {session}");
    let written = std::fs::create_dir_all(&dir).and_then(|()| {
        let text = serde_json::to_string_pretty(&recording).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(format!("{session}.json")), text)
    });
    if let Err(e) = written {
        tracing::warn!(%session, error = %e, "could not save the session's recording");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use mm_agent::{AuthorConfig, Feedback, RecordedClient};
    use mm_graph::Palette;

    use super::*;
    use crate::SpecStore;

    #[tokio::test]
    async fn a_saved_session_plays_back_the_same() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let palette = Palette::pinned().unwrap();
        let specs = SpecStore::load(&root.join("specs"), &palette).unwrap();
        let spec = specs.get("chw_plant_reset").unwrap();
        let original: Recording = serde_json::from_slice(
            &std::fs::read(root.join("fixtures/recordings/chw_plant_reset.json")).unwrap(),
        )
        .unwrap();
        let config = AuthorConfig {
            model: original.model.clone(),
            effort: None,
            max_output_tokens: 1000,
            max_attempts: 5,
            feedback: Feedback::WithVectors,
        };
        let run = |rec: Recording| {
            let client = RecordedClient::new(rec, Duration::ZERO);
            let (palette, spec, config) = (&palette, spec, &config);
            async move {
                mm_agent::author(&client, palette, &spec.task, &spec.vectors, &[], config).await
            }
        };
        let trace = run(original.clone()).await;
        let verified = trace.verified.as_ref().map(|v| v.content_id.clone());
        assert!(verified.is_some(), "the committed recording verifies");

        let dir = std::env::temp_dir().join(format!("mm-recording-{}", std::process::id()));
        save(Some(&dir), "author-1", &trace);
        let saved: Recording =
            serde_json::from_slice(&std::fs::read(dir.join("recordings/author-1.json")).unwrap())
                .unwrap();
        assert_eq!(saved.replies, original.replies);
        assert_eq!(saved.note, "mm-server live session author-1");
        // Played back, the saved recording verifies the same draft.
        let again = run(saved).await;
        assert_eq!(again.verified.map(|v| v.content_id), verified);
        let _ = std::fs::remove_dir_all(dir);
    }
}
