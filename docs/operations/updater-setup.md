# Signed Updater Setup (Windows First)

This guide explains how to enable signed updates safely once you have:

- Code signing certificate (`.pfx`)
- Publisher name
- Timestamp server URL
- Update hosting URL (S3, R2, or Azure)

## 1. Add Signing Secrets (Local or CI)

Set these environment variables in your build machine or CI:

- `ALETHEIA_SIGN_PFX_PATH`
- `ALETHEIA_SIGN_PFX_PASSWORD`
- `ALETHEIA_SIGN_PUBLISHER`
- `ALETHEIA_SIGN_TIMESTAMP_URL`

## 2. Update Endpoint

Decide your update hosting base URL, then replace the updater endpoint in:

- `src-tauri/tauri.conf.json`

Example endpoint pattern:

```
https://updates.yourdomain.com/aletheia/{{target}}/{{current_version}}
```

## 3. Generate Update Keys

Use the Tauri CLI to generate updater keys:

```
tauri signer generate
```

Store the private key outside source control. Add the public key to `tauri.conf.json` once ready.

## 4. Release Flow

1. Build signed Windows installer.
2. Upload update artifacts to your hosting bucket.
3. Publish the latest update manifest.
4. Run rollback rehearsal on a clean Windows machine.

## 5. Rollback Rehearsal

On Windows:

1. Install current version.
2. Apply update.
3. Roll back to previous version.
4. Verify the app opens and the database is intact.

Record the rollback evidence in the device acceptance checklist.
