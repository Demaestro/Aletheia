# vMix Integration

Aletheia talks to vMix through the vMix HTTP Web API. The production default is intentionally local-only:

- Endpoint: `http://127.0.0.1:8088/api/`
- Title input: `Aletheia Scripture.gtzip`
- Verse field: `Headline.Text`
- Reference field: `Description.Text`
- Overlay channel: `2`

## vMix Setup

1. Open vMix Settings, then enable the Web Controller/API from the Web tab.
2. Confirm the vMix web interface is reachable from the operator machine at `http://127.0.0.1:8088/api/`.
3. Add a GT title or XAML title named `Aletheia Scripture.gtzip`, or update the adapter config later when the settings UI becomes editable.
4. In the title editor, expose these text fields:
   - `Headline.Text` for the verse body.
   - `Description.Text` for the scripture reference.
5. In Aletheia, open Integrations, run `Check vMix`, send `Preview title`, then use `Take live` only after destinations are armed.

## Command Mapping

Aletheia uses vMix commands that match the official vMix API query-string model:

- `SetText` with `Input`, `SelectedName`, and `Value` updates title fields.
- `PreviewOverlayInput2` stages the title on the configured overlay preview.
- `OverlayInput2In` takes the title live.
- `OverlayInput2Out` clears the scripture overlay.

Before updating title text, the adapter sends `PauseRender`; after both text fields are updated, it sends `ResumeRender`. This reduces visible field-by-field flicker during fast scripture changes.

## Security Policy

The adapter blocks public remote addresses. By default it only connects to loopback. Private LAN vMix hosts will require an explicit operator/admin setting before use. This keeps local worship automation from becoming a generic HTTP request tool.

No vMix secrets are stored in the current adapter. If future deployments require authenticated network bridges, credentials must go through the desktop secure-storage boundary and must be redacted from logs and support bundles.

## Production Checklist

- Verify the title input exists before service.
- Rehearse preview, live, and clear during pre-service.
- Keep the vMix Web API bound to loopback where possible.
- Use a dedicated overlay channel for scripture so Aletheia never interferes with lower thirds, sermon notes, or lyrics.
- Record successful live and clear actions in the local audit log.
