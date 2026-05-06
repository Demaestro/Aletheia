# Aletheia — Production Gaps: Step-by-Step Setup Guide

This guide walks you through the six things that cannot be solved by writing
more code alone. They each require external tools, files, or hardware. Each
section tells you exactly what to do, why it matters, and what to check
when you are done.

---

## 1  Real STT (Speech-to-Text) Execution Wiring

**What the gap is**  
The app currently simulates transcripts from hard-coded production data. No
actual microphone audio is captured. The STT routing policy, language
detection, and detection pipeline are all real and correct — they just have
no live audio to process.

**Why it is not done in code yet**  
Real STT requires the `whisper.cpp` native library to be compiled and linked,
plus a crate (`whisper-rs`) that wraps it, plus a platform audio-capture
crate (`cpal`). These involve C/C++ compile steps and are too large to include
before you have confirmed the target machine specs and microphone setup.

**Step-by-step fix**

1. **Install Rust build tools if not already present**  
   Open a command prompt and run:
   ```
   rustup update stable
   ```

2. **Install the CMake build tool** (needed to compile whisper.cpp)  
   Download from https://cmake.org/download/ and install.  
   Verify: `cmake --version`

3. **Add the `whisper-rs` and `cpal` crates** to
   `crates/aletheia-stt/Cargo.toml`:
   ```toml
   cpal = "0.15"
   whisper-rs = { version = "0.11", features = ["whisper-cpp-default"] }
   ```

4. **Download a Whisper model file**  
   For English: `ggml-small.en.bin` (466 MB)
   ```
   curl -L -o whisper-base-en.bin \
     "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
   ```
   Place the file in the app's offline-assets directory (shown on the Health
   screen under "App data directory").

5. **Install the model through the app**  
   In Aletheia → Health → Offline Model Packs, click **Install from file** next
   to the "whisper-base-en" asset. Point it at the file you just downloaded.
   The app will verify the SHA-256 checksum and mark it installed.

6. **Implement the audio capture loop** in
   `crates/aletheia-stt/src/lib.rs`  
   Replace the stub `SttRouter::route()` return type with a real channel-based
   loop that:  
   - Opens the default input device with `cpal`  
   - Buffers 30-second chunks of 16-kHz mono f32 samples  
   - Passes each chunk to `WhisperContext::full()` from `whisper-rs`  
   - Emits a `TranscriptSegment` for each result over a `tokio::sync::mpsc`
     channel

7. **Wire the channel into the Tauri backend**  
   In `src-tauri/src/lib.rs` inside the `setup` closure, spawn a `tokio` task
   that reads from the STT channel and appends segments to a shared
   `Mutex<Vec<TranscriptSegmentDto>>`, then emits a Tauri event so the
   frontend can `listen()` for new segments.

**What success looks like**  
The Transcript screen updates in real time as the pastor speaks. The AI
Detection screen shows live candidates without needing to press "Analyse".

---

## 2  Licensed Offline Model Packs (multilingual Whisper)

**What the gap is**  
The Health screen shows six STT packs as "pending install":
Hausa, Twi, Swahili, Xhosa, Spanish, and French. The install flow and
checksum validation are fully built — you just need the files.

**Why it is not done in code yet**  
The model files are 75–300 MB each. They cannot be bundled with the app.
Each language requires operator-approved download.

**Step-by-step fix**

1. **Download the multilingual Whisper base model** (handles all six languages)
   ```
   curl -L -o ggml-base-multilingual.bin \
     "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
   ```
   This single 142 MB file covers all languages.

2. **Get the SHA-256 checksum of the file**  
   On Windows:
   ```
   certutil -hashfile ggml-base-multilingual.bin SHA256
   ```

3. **Update the expected checksums** in `crates/aletheia-ops/src/lib.rs`  
   Find the `production_offline_asset_manifest()` function.  
   For each language pack asset, set `checksum_sha256` to the hex string from
   step 2.

4. **Install from file in the app**  
   In Aletheia → Health → Offline Model Packs, use **Install from file** for
   each language pack. All six packs can share the same model file — point them
   all at `ggml-base-multilingual.bin`. The app will copy and verify each one.

5. **Check the Health screen**  
   All six language packs should move from "pending" to "installed". The
   Production Readiness score should increase.

---

## 3  Hardware Acceptance on Real Devices

**What the gap is**  
The acceptance checklist UI is fully built and stores results in SQLite. The
acceptance steps for vMix, OBS, EasyWorship, HDMI, NDI, and ProPresenter are
defined. You just need to run them on the actual production machine.

**Step-by-step fix (run on production day, not in development)**

1. **Connect all hardware** — HDMI capture, NDI sender, audio interface.

2. **Start each integration** — open vMix, OBS, EasyWorship on the booth PC.

3. **Open Aletheia → Health → Hardware Acceptance Checklist**.

4. **For each device**, work through every required step:
   - Read the "Expected" description
   - Perform the physical action (e.g. "Verify text appears on confidence monitor")
   - Click **Pass** or **Fail**
   - Add a note if the step partially passed

5. **Green-light the rehearsal**  
   Once all required steps for all devices show as Passed, the Production
   Readiness gate will change from "blocked" to "ready".

6. **Export a support bundle** before going live. This records the acceptance
   receipts and can be reviewed if something goes wrong later.

---

## 4  Signed Installer and Updater Pipeline

**What the gap is**  
The Tauri updater is built and the pubkey is configured. The updater endpoint
in `tauri.conf.json` is a placeholder. The installer is unsigned.

**Why it matters**  
Windows and macOS will show security warnings for unsigned executables.
The auto-updater will not work until a real endpoint is serving the update
JSON.

**Step-by-step fix**

### 4a — Code-signing certificate

1. Purchase an EV (Extended Validation) code-signing certificate from a CA
   such as DigiCert, Sectigo, or Certum. Cost: ~$350/year.

2. Follow the CA's instructions to export a `.pfx` (Windows) or `.p12`
   (macOS) file.

3. Set the certificate in your build environment:
   ```
   # Windows (PowerShell)
   $env:TAURI_SIGNING_PRIVATE_KEY = "path\to\key.pfx"
   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your-password"
   ```

4. Run `npm run tauri build` — Tauri will sign the installer automatically.

### 4b — Updater endpoint (GitHub Releases)

1. Create a **private GitHub repository** for releases (or use the existing
   one if you have it).

2. Create a GitHub Actions workflow at `.github/workflows/release.yml` that:
   - Triggers on `push --tags`
   - Runs `npm run tauri build` on Windows and macOS runners
   - Publishes the `.msi`, `.dmg`, and update JSON to a GitHub Release

3. Update `tauri.conf.json` → `plugins.updater.endpoints`:
   ```json
   "endpoints": [
     "https://github.com/YOUR_ORG/aletheia-releases/releases/latest/download/update-{{target}}-{{current_version}}.json"
   ]
   ```

4. Change `"active": false` to `"active": true` in the same `updater` block.

5. Tag a release: `git tag v0.1.0 && git push --tags`  
   The workflow builds, signs, and publishes. The next time a user opens
   Aletheia, the updater will find the new version.

---

## 5  Cloud STT Fallback

**What the gap is**  
When offline Whisper packs are not installed and Data Miser is off, the STT
routing policy selects `cloud` mode — but there is no cloud adapter
implemented. The adapter stub returns "offline".

**Step-by-step fix**

1. **Get an API key** from one of:
   - OpenAI Whisper API (https://platform.openai.com) — most language-aware
   - AssemblyAI (https://www.assemblyai.com) — good multilingual support

2. **Store the key in the vault** from Aletheia → Integrations → Secrets:
   Label: `stt-cloud-api-key`

3. **Add `reqwest` to `crates/aletheia-stt/Cargo.toml`**:
   ```toml
   reqwest = { version = "0.12", features = ["json", "multipart"] }
   ```

4. **Implement a `CloudSttAdapter`** in
   `crates/aletheia-stt/src/cloud.rs`:
   ```rust
   // Reads the API key from the OS vault using the keyring crate,
   // then POSTs the audio buffer to the provider's transcription endpoint.
   // Returns a Vec<TranscriptSegment> on success.
   ```

5. **Wire into the `SttRouter`** — when `SttRoutingPolicy::mode` is `Cloud`,
   call `CloudSttAdapter::transcribe()` instead of the local stub.

6. **Test with Data Miser off** and no offline packs installed.  
   The STT adapter status should change from "degraded" to "ready" on the
   AI Detection screen.

---

## 6  Calibration Dataset Pipeline (ongoing use)

**What the gap is**  
The calibration schema, storage methods, Tauri commands, and TypeScript API
are all fully built and ready. The pipeline just needs operator input — you
start it by reviewing detection results during live services.

**How to start collecting samples**

1. **After each service**, open Aletheia → Transcript.

2. For each AI-detected candidate the operator reviewed, call
   `recordCalibrationSample()` from the TypeScript API with:
   - `language` — the BCP-47 code ("en", "ha", "sw", etc.)
   - `transcriptText` — the transcript snippet that triggered the detection
   - `expectedRef` — the correct scripture reference, or `null` if none
   - `outcome` — `"confirmed"` (AI was right), `"corrected"` (AI was wrong
     but scripture was real), or `"rejected"` (no scripture at all)
   - `detectedRef` — what Aletheia said

   This is already wired up through the `record_calibration_sample` Tauri
   command.

3. **Check the report** with `getCalibrationReport()`.  
   The `precision` field appears once you have 5 or more positive decisions.
   Target: 95% precision, 90% recall (matching the accuracy gate in the
   local rehearsal).

4. **Use the data to improve aliases** — if certain languages miss references,
   add more alias entries to `crates/aletheia-detection/src/aliases.rs`.

**What success looks like**  
After 20–30 services, the calibration report shows ≥ 95% precision across
all six languages and the accuracy gate in the local rehearsal passes green.

---

## Summary Table

| Gap | What you do | Time needed |
|-----|-------------|-------------|
| Real STT wiring | Download whisper.cpp model, implement audio loop | 2–4 hours |
| Multilingual packs | Download model file, install from Health screen | 30 minutes |
| Hardware acceptance | Run checklist on production day | 20 minutes |
| Signed installer | Buy EV cert, set up GitHub Actions release pipeline | 2–3 hours |
| Cloud STT fallback | Get API key, implement `CloudSttAdapter` | 2–3 hours |
| Calibration data | Review AI results after each service | Ongoing |

Everything else — OBS, OSC, EasyWorship, vMix, plugin verification, offline
scripture search, audit logging, service profiles, production readiness
scoring, booth pack export, support bundles — is fully built and integrated.
