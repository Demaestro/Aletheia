//! Aletheia audio service bootstrap.
//!
//! This binary is intentionally a deterministic harness for the first build
//! slice. Hardware capture, STT sidecars, and Tauri command registration land
//! after the service contracts are stable.

use aletheia_audio_ingest::{AudioDeviceConfig, AudioIngestService};
use aletheia_core::{DeviceId, InMemoryEventLog, ServiceSessionId, events::EventLog};
use aletheia_detection::{ReferenceKeywordDetector, ScriptureDetector, TranscriptSegment};
use aletheia_output::OutputScene;
use aletheia_vad::HybridVad;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_id = ServiceSessionId::new("sunday-am-rehearsal")?;
    let device_id = DeviceId::new("focusrite-usb-pulpit")?;
    let config = AudioDeviceConfig::pulpit_mic(device_id, "Focusrite USB Pulpit Mic");
    let mut service = AudioIngestService::new(
        session_id.clone(),
        config,
        HybridVad::default(),
        InMemoryEventLog::default(),
    );

    service.start()?;
    let simulated_speech_frame = vec![0.08_f32; 960];
    let vad_decision = service.process_pcm_frame(&simulated_speech_frame)?;

    let transcript = TranscriptSegment {
        id: "seg-0001".to_string(),
        session_id: session_id.clone(),
        started_at_ms: 1_240,
        ended_at_ms: 3_800,
        speaker_label: Some("Pastor Daniel".to_string()),
        language: "English".to_string(),
        text: "Though I walk through the valley of the shadow of death, I will fear no evil."
            .to_string(),
        confidence: 0.94,
        adapter: "offline-whisper".to_string(),
        latency_ms: 420,
    };

    let detector = ReferenceKeywordDetector;
    let candidates = detector.detect(&transcript, &[]);
    let selected = candidates.first();
    let output_scene = selected.map(|candidate| {
        OutputScene::scripture(
            "scene-0001",
            session_id.clone(),
            &candidate.reference,
            &candidate.translation,
            "Yea, though I walk through the valley of the shadow of death, I will fear no evil.",
            "film-credit-lower-third",
        )
    });

    println!("Aletheia audio service booted");
    println!(
        "vad speech={} confidence={:.2}",
        vad_decision.speech_detected, vad_decision.confidence
    );
    println!("events={}", service.event_log().all().len());
    if let Some(scene) = output_scene {
        println!(
            "candidate={} layers={}",
            scene.reference,
            scene.layers.len()
        );
    } else {
        println!("candidate=none");
    }

    Ok(())
}
