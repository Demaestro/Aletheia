# Plugin Signing Enforcement

Plugins must be signed with Ed25519 and verified before enabling in production mode.

## Requirements

- Signed manifest JSON with payload + signature envelope
- Trusted key id allowlist configured outside source control
- No wildcard network hosts
- Explicit capability list

## Verification Flow

1. Load manifest JSON.
2. Validate payload policy (id, name, entrypoint, allowed hosts).
3. Verify Ed25519 signature.
4. If verification fails, plugin stays disabled.

## Operational Checklist

- Store trusted key ids in a secure config or vault.
- Audit every plugin enable/disable action.
- Re-verify signatures after updates.
