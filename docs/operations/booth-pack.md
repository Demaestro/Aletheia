# Booth Compatibility Pack

Aletheia can export a booth compatibility pack from the current preview scripture. The pack is designed for real AV operators who need a practical fallback even when a vendor API, cloud link, or Sunday network fails.

## What The Pack Contains

- `obs/aletheia-browser-source.html`: a transparent 1920x1080 lower-third browser source for OBS.
- `obs/current-verse.json`: the current scripture payload for scripts or custom OBS automation.
- `easyworship/current-verse.txt`: a plain text scripture handoff for EasyWorship import or watch-folder workflows.
- `propresenter/playlist-cue.json`: a cue payload for ProPresenter API scripts or rehearsal import.
- `vmix/setup.md`: exact title input, text fields, port, and rehearsal steps for vMix.
- `ndi/layers.json`: expected verse, reference, and context-card layers for NDI output validation.
- `hdmi/output-window.json`: safe-area and secondary-display expectations for projector or capture output.
- `companion/buttons.json`: Stream Deck or Companion-style button intents.
- `osc/cues.json`: OSC namespace and cue payloads.

## Operator Flow

1. Open the Aletheia desktop app.
2. Confirm the best scripture candidate is in Preview.
3. Open Integrations.
4. Press **Export booth pack**.
5. Open the generated folder shown in the app.
6. Add `obs/aletheia-browser-source.html` as an OBS browser source at 1920x1080.
7. Import or watch `easyworship/current-verse.txt` in EasyWorship.
8. Configure vMix from `vmix/setup.md`, then use **Check vMix**, **Preview title**, **Take live**, and **Clear** inside Aletheia.
9. Run the device acceptance checklist before service starts.

## Safety Model

- AI can prepare Preview, but Live requires armed destinations.
- The export contains no provider keys, API secrets, or raw credentials.
- The pack is safe to share with the booth team for rehearsal, but treat transcript content as service-sensitive.
- Keep Data Miser enabled on unstable 3G/4G networks unless cloud enhancement has been rehearsed.

## Production Notes

The pack is intentionally file-based. File exports are boring in the best way: they keep working when vendor APIs are unavailable, when the booth network is locked down, or when volunteers need a fallback they can understand by inspection.

Real device acceptance still has to be completed on the Windows booth machine with the actual vMix, OBS, EasyWorship, ProPresenter, NDI, and HDMI hardware chain.
