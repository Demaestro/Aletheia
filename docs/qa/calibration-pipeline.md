# Calibration Pipeline (Offline Accuracy)

This pipeline validates that local speech-to-text and scripture detection meet the 95% precision gate.

## 1. Dataset Format

Each sample is a JSON line:

```
{
  "id": "sample-001",
  "language": "Hausa",
  "transcript": "Mu bude Romawa 8:28 tare da ikilisiya.",
  "expectedReference": "Romans 8:28"
}
```

## 2. Rehearsal Coverage

Minimum recommended:

- 10+ hours of real church audio
- Background music bleed
- Interpreter overlap
- Code switching
- Multiple microphone types

## 3. Scoring

Metrics to record:

- Precision / Recall
- Time to first detection
- False live‑risk events
- Operator override rate

## 4. Gate Policy

- Auto‑preview only when precision ≥ 95% for the language route.
- Live output is always manual unless the church explicitly changes policy.

## 5. Storage

Store dataset and result reports outside the app source tree, but keep a redacted summary for audit.
