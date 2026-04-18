use aletheia_core::now_ms;
use aletheia_detection::{
    AccuracyFixture, ReferenceKeywordDetector, evaluate_accuracy_fixtures,
};
use crate::dto::*;
use crate::DesktopState;

/// Runs the offline scripture-detection accuracy fixture suite and returns a
/// structured report the UI can display as a pre-service readiness check.
pub fn evaluate_accuracy_fixtures_cmd(state: &DesktopState) -> Result<LocalRehearsalReportDto, String> {
    let start = std::time::Instant::now();
    let detector = ReferenceKeywordDetector;
    let fixtures = rehearsal_fixtures();
    let total = fixtures.len() as u16;

    let evaluation = evaluate_accuracy_fixtures(&detector, &fixtures);

    let true_positives = evaluation.true_positives;
    let false_positives = evaluation.false_positives;
    let false_negatives = evaluation.false_negatives;
    let passed = true_positives;

    let mut steps: Vec<LocalRehearsalStepDto> = fixtures.iter().map(|fixture| {
        let elapsed = start.elapsed().as_millis() as u32;
        // Re-run each fixture individually so we get per-step pass/fail
        let single = evaluate_accuracy_fixtures(&detector, &std::slice::from_ref(fixture));
        let (step_state, step_detail) = if fixture.expected_reference.is_some() {
            if single.true_positives > 0 {
                ("healthy", format!("Detected expected reference in \"{}\"", truncate(fixture.text, 60)))
            } else if single.false_negatives > 0 {
                ("degraded", format!("Missed expected reference in \"{}\"", truncate(fixture.text, 60)))
            } else {
                ("degraded", format!("No detection for \"{}\"", truncate(fixture.text, 60)))
            }
        } else {
            // negative fixture — should produce no detection
            if single.false_positives > 0 {
                ("degraded", format!("False positive for \"{}\"", truncate(fixture.text, 60)))
            } else {
                ("healthy", format!("Correctly suppressed false positive for \"{}\"", truncate(fixture.text, 60)))
            }
        };
        LocalRehearsalStepDto {
            label: format!("[{}] {}", fixture.language, fixture.id),
            state: step_state.to_string(),
            detail: step_detail.to_string(),
            duration_ms: elapsed,
        }
    }).collect();

    // Add a store connectivity check
    let store_start = start.elapsed().as_millis() as u32;
    let store_step = match state.lock_store() {
        Ok(store) => {
            let count: i64 = store
                .connection()
                .query_row("SELECT COUNT(*) FROM verses", [], |r| r.get(0))
                .unwrap_or(0);
            LocalRehearsalStepDto {
                label: "Scripture library".to_string(),
                state: if count > 1000 { "healthy" } else { "degraded" }.to_string(),
                detail: format!("{count} verses in local database"),
                duration_ms: start.elapsed().as_millis() as u32 - store_start,
            }
        }
        Err(e) => LocalRehearsalStepDto {
            label: "Scripture library".to_string(),
            state: "offline".to_string(),
            detail: format!("DB unavailable: {e}"),
            duration_ms: 0,
        },
    };
    steps.push(store_step);

    let all_healthy = steps.iter().all(|s| s.state == "healthy");
    let overall_state = if all_healthy { "pass" } else { "degraded" }.to_string();

    // Write proof path: save report JSON next to the DB
    let proof_path = {
        let dir = state.database_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let path = dir.join("rehearsal-proof.json");
        let summary = serde_json::json!({
            "generatedAtMs": now_ms(),
            "precision": evaluation.precision,
            "recall": evaluation.recall,
            "truePositives": true_positives,
            "falsePositives": false_positives,
            "falseNegatives": false_negatives,
            "total": total,
        });
        let _ = std::fs::write(&path, summary.to_string());
        path.display().to_string()
    };

    Ok(LocalRehearsalReportDto {
        generated_at_ms: now_ms(),
        state: overall_state,
        passed,
        total: total + 1, // +1 for the store check
        proof_path,
        steps,
    })
}

/// Single-step rehearsal probe (called by the UI for per-step checks).
pub fn local_rehearsal_step(_state: &DesktopState) -> Result<LocalRehearsalStepDto, String> {
    let start = std::time::Instant::now();
    // Run the full detection suite as a single probe
    let detector = ReferenceKeywordDetector;
    let fixtures = rehearsal_fixtures();
    let evaluation = evaluate_accuracy_fixtures(&detector, &fixtures);
    let ok = evaluation.precision >= 0.9 && evaluation.recall >= 0.85;
    Ok(LocalRehearsalStepDto {
        label: "Scripture detection accuracy".to_string(),
        state: if ok { "healthy" } else { "degraded" }.to_string(),
        detail: format!(
            "Precision {:.0}%, Recall {:.0}% over {} fixtures",
            evaluation.precision * 100.0,
            evaluation.recall * 100.0,
            fixtures.len()
        ),
        duration_ms: start.elapsed().as_millis() as u32,
    })
}

fn rehearsal_fixtures() -> Vec<AccuracyFixture> {
    vec![
        AccuracyFixture { id: "english-romans",   language: "English", text: "Please open Romans 8:28.",                           expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "hausa-romans",     language: "Hausa",   text: "Mu bude Romawa 8:28 tare da ikilisiya.",             expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "twi-romans",       language: "Twi",     text: "Momma yenhwɛ Romafo 8:28 ansa na yebɔ mpae.",       expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "swahili-romans",   language: "Swahili", text: "Tufungue Warumi 8:28 pamoja na kanisa.",             expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "xhosa-romans",     language: "Xhosa",   text: "Masivule KwabaseRoma 8:28 namhlanje.",               expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "spanish-romans",   language: "Spanish", text: "Abramos Romanos 8:28 juntos.",                       expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "french-romans",    language: "French",  text: "Ouvrons Romains 8:28 ensemble.",                     expected_reference: Some("Romans 8:28") },
        AccuracyFixture { id: "english-psalm23",  language: "English", text: "Turn to Psalm 23 verse 4.",                         expected_reference: Some("Psalm 23:4")  },
        AccuracyFixture { id: "negative-prayer",  language: "English", text: "We will pray after the song.",                      expected_reference: None },
        AccuracyFixture { id: "negative-generic", language: "English", text: "The pastor will speak now.",                        expected_reference: None },
    ]
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}
