//! SQLite persistence boundary for Aletheia.

use aletheia_core::{AuditAction, EventEnvelope, TimestampMs};
use rusqlite::{Connection, OptionalExtension, params};

/// Current schema version.
pub const SCHEMA_VERSION: i64 = 9;

/// SQLite-backed local store.
pub struct AletheiaStore {
    connection: Connection,
}

impl AletheiaStore {
    /// Opens an in-memory store for tests and deterministic demos.
    pub fn open_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    /// Opens or creates a SQLite database file.
    ///
    /// Applies connection-level PRAGMAs for durability and concurrency
    /// (WAL journal, normal synchronous mode, foreign key enforcement),
    /// then runs migrations.  After migrating, the on-disk schema version is
    /// checked: if it is newer than `SCHEMA_VERSION` this returns
    /// `StoreError::SchemaTooNew` so the caller surfaces a clear error instead
    /// of silently operating on an incompatible schema.
    pub fn open_file(path: impl AsRef<std::path::Path>) -> StoreResult<Self> {
        let connection = Connection::open(path)?;
        // Apply connection-level settings before any DDL.
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous   = NORMAL;
             PRAGMA foreign_keys  = ON;",
        )?;
        let store = Self { connection };
        store.migrate()?;
        let on_disk = store.schema_version()?;
        if on_disk > SCHEMA_VERSION {
            return Err(StoreError::SchemaTooNew {
                on_disk,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(store)
    }

    /// Applies idempotent migrations.
    pub fn migrate(&self) -> StoreResult<()> {
        self.connection.execute_batch(SCHEMA)?;
        self.connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Returns the SQLite schema version.
    pub fn schema_version(&self) -> StoreResult<i64> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// Adds a scripture translation row.
    pub fn insert_translation(&self, translation: &TranslationRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO translations (id, name, language, license, offline_ready)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               language = excluded.language,
               license = excluded.license,
               offline_ready = excluded.offline_ready",
            params![
                translation.id,
                translation.name,
                translation.language,
                translation.license,
                translation.offline_ready
            ],
        )?;
        Ok(())
    }

    /// Inserts one verse and its FTS row.
    pub fn insert_verse(&self, verse: &VerseRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO scripture_verses
               (translation_id, book, chapter, verse, text)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(translation_id, book, chapter, verse)
             DO UPDATE SET text = excluded.text",
            params![
                verse.translation_id,
                verse.book,
                verse.chapter,
                verse.verse,
                verse.text
            ],
        )?;
        self.connection.execute(
            "DELETE FROM scripture_verses_fts
             WHERE translation_id = ?1 AND book = ?2 AND chapter = ?3 AND verse = ?4",
            params![verse.translation_id, verse.book, verse.chapter, verse.verse],
        )?;
        self.connection.execute(
            "INSERT INTO scripture_verses_fts
               (translation_id, book, chapter, verse, text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                verse.translation_id,
                verse.book,
                verse.chapter,
                verse.verse,
                verse.text
            ],
        )?;
        Ok(())
    }

    /// Deletes a translation and all its verses (verses, FTS rows, registry).
    /// Returns the number of verse rows deleted. Atomic: rolled back on error.
    pub fn delete_translation(&self, translation_id: &str) -> StoreResult<u32> {
        let tx = self.connection.unchecked_transaction()?;
        let removed = tx.execute(
            "DELETE FROM scripture_verses WHERE translation_id = ?1",
            params![translation_id],
        )?;
        tx.execute(
            "DELETE FROM scripture_verses_fts WHERE translation_id = ?1",
            params![translation_id],
        )?;
        tx.execute(
            "DELETE FROM translations WHERE id = ?1",
            params![translation_id],
        )?;
        tx.commit()?;
        Ok(removed as u32)
    }

    /// Counts verses for a translation (used to detect partial seed states).
    pub fn count_verses_for_translation(&self, translation_id: &str) -> StoreResult<i64> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM scripture_verses WHERE translation_id = ?1",
            params![translation_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Finds a single verse by canonical reference.
    pub fn find_verse(
        &self,
        translation_id: &str,
        book: &str,
        chapter: u16,
        verse: u16,
    ) -> StoreResult<Option<VerseRecord>> {
        self.connection
            .query_row(
                "SELECT translation_id, book, chapter, verse, text
                 FROM scripture_verses
                 WHERE translation_id = ?1 AND book = ?2 AND chapter = ?3 AND verse = ?4",
                params![translation_id, book, chapter, verse],
                VerseRecord::from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Runs local phrase search using SQLite FTS5.
    pub fn search_phrase(&self, phrase: &str, limit: u16) -> StoreResult<Vec<VerseRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT translation_id, book, chapter, verse, text
             FROM scripture_verses_fts
             WHERE scripture_verses_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![phrase, limit], VerseRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Records an audit action in the local tamper-evident log foundation.
    pub fn insert_audit_event(&self, event: &AuditEventRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO audit_log
               (timestamp_ms, action, actor, detail, previous_hash, event_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.timestamp_ms,
                event.action,
                event.actor,
                event.detail,
                event.previous_hash,
                event.event_hash
            ],
        )?;
        Ok(())
    }

    /// Persists a domain event envelope as JSON text supplied by the caller.
    pub fn insert_domain_event_json(
        &self,
        envelope: &EventEnvelope,
        event_json: &str,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO domain_events (sequence, timestamp_ms, event_json)
             VALUES (?1, ?2, ?3)",
            params![envelope.sequence, envelope.timestamp_ms, event_json],
        )?;
        Ok(())
    }

    /// Creates or updates a non-secret integration configuration.
    pub fn upsert_integration_config(&self, config: &IntegrationConfigRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO integration_configs
               (id, kind, display_name, enabled, config_json, secret_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               kind = excluded.kind,
               display_name = excluded.display_name,
               enabled = excluded.enabled,
               config_json = excluded.config_json,
               secret_ref = excluded.secret_ref",
            params![
                config.id,
                config.kind,
                config.display_name,
                config.enabled,
                config.config_json,
                config.secret_ref
            ],
        )?;
        Ok(())
    }

    /// Loads an integration configuration by id.
    pub fn get_integration_config(&self, id: &str) -> StoreResult<Option<IntegrationConfigRecord>> {
        self.connection
            .query_row(
                "SELECT id, kind, display_name, enabled, config_json, secret_ref
                 FROM integration_configs
                 WHERE id = ?1",
                params![id],
                IntegrationConfigRecord::from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Records an integration delivery or health event.
    pub fn insert_integration_event(&self, event: &IntegrationEventRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO integration_events
               (timestamp_ms, integration_id, severity, action, detail, receipt_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.timestamp_ms,
                event.integration_id,
                event.severity,
                event.action,
                event.detail,
                event.receipt_json
            ],
        )?;
        Ok(())
    }

    /// Returns the most recent integration events in newest-first order.
    pub fn recent_integration_events(
        &self,
        limit: u16,
    ) -> StoreResult<Vec<IntegrationEventRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT timestamp_ms, integration_id, severity, action, detail, receipt_json
             FROM integration_events
             ORDER BY timestamp_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], IntegrationEventRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Ensures an offline asset state row exists (insert-only).
    pub fn ensure_offline_asset_state(&self, record: &OfflineAssetStateRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO offline_assets (id, state, checksum, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                record.id,
                record.state,
                record.checksum,
                record.updated_at_ms
            ],
        )?;
        Ok(())
    }

    /// Updates an offline asset state row.
    pub fn update_offline_asset_state(&self, record: &OfflineAssetStateRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO offline_assets (id, state, checksum, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               state = excluded.state,
               checksum = excluded.checksum,
               updated_at_ms = excluded.updated_at_ms",
            params![
                record.id,
                record.state,
                record.checksum,
                record.updated_at_ms
            ],
        )?;
        Ok(())
    }

    /// Lists offline asset states.
    pub fn list_offline_asset_states(&self) -> StoreResult<Vec<OfflineAssetStateRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, state, checksum, updated_at_ms
             FROM offline_assets
             ORDER BY id",
        )?;
        let rows = statement.query_map([], OfflineAssetStateRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Records a device acceptance receipt for hardware rehearsal.
    pub fn insert_device_acceptance_receipt(
        &self,
        receipt: &DeviceAcceptanceReceiptRecord,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO device_acceptance_receipts
               (device_id, step_label, passed, note, evidence_path, recorded_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                receipt.device_id,
                receipt.step_label,
                receipt.passed,
                receipt.note,
                receipt.evidence_path,
                receipt.recorded_at_ms
            ],
        )?;
        Ok(())
    }

    /// Lists device acceptance receipts for a given device.
    pub fn list_device_acceptance_receipts(
        &self,
        device_id: &str,
    ) -> StoreResult<Vec<DeviceAcceptanceReceiptRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT device_id, step_label, passed, note, evidence_path, recorded_at_ms
             FROM device_acceptance_receipts
             WHERE device_id = ?1
             ORDER BY recorded_at_ms DESC, id DESC",
        )?;
        let rows =
            statement.query_map(params![device_id], DeviceAcceptanceReceiptRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Upserts a service profile.
    pub fn upsert_service_profile(&self, profile: &ServiceProfileRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO service_profiles
               (id, name, languages_json, output_policy, is_active, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               languages_json = excluded.languages_json,
               output_policy = excluded.output_policy,
               is_active = excluded.is_active,
               updated_at_ms = excluded.updated_at_ms",
            params![
                profile.id,
                profile.name,
                profile.languages_json,
                profile.output_policy,
                profile.is_active as i64,
                profile.created_at_ms,
                profile.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Returns the active service profile (the one with `is_active = 1`).
    pub fn get_active_service_profile(&self) -> StoreResult<Option<ServiceProfileRecord>> {
        self.connection
            .query_row(
                "SELECT id, name, languages_json, output_policy, is_active, created_at_ms, updated_at_ms
                 FROM service_profiles
                 WHERE is_active = 1
                 LIMIT 1",
                [],
                ServiceProfileRecord::from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Returns all saved service profiles in creation order.
    pub fn list_service_profiles(&self) -> StoreResult<Vec<ServiceProfileRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, name, languages_json, output_policy, is_active, created_at_ms, updated_at_ms
             FROM service_profiles
             ORDER BY created_at_ms ASC",
        )?;
        let rows = stmt.query_map([], ServiceProfileRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Sets one profile as active and deactivates all others.
    pub fn set_active_service_profile(&self, id: &str) -> StoreResult<()> {
        self.connection
            .execute("UPDATE service_profiles SET is_active = 0", [])?;
        self.connection.execute(
            "UPDATE service_profiles SET is_active = 1, updated_at_ms = ?1 WHERE id = ?2",
            params![aletheia_core::now_ms(), id],
        )?;
        Ok(())
    }

    /// Deletes a service profile by id. The active profile cannot be deleted.
    pub fn delete_service_profile(&self, id: &str) -> StoreResult<usize> {
        let n = self.connection.execute(
            "DELETE FROM service_profiles WHERE id = ?1 AND is_active = 0",
            params![id],
        )?;
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Trusted plugins registry (v6)
    // -----------------------------------------------------------------------

    /// Inserts or replaces a verified plugin in the trust registry.
    pub fn upsert_trusted_plugin(&self, plugin: &TrustedPluginRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO trusted_plugins
               (id, name, version, key_id, digest, capabilities_json, enabled, trusted_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               version = excluded.version,
               key_id = excluded.key_id,
               digest = excluded.digest,
               capabilities_json = excluded.capabilities_json,
               enabled = excluded.enabled,
               trusted_at_ms = excluded.trusted_at_ms",
            params![
                plugin.id,
                plugin.name,
                plugin.version,
                plugin.key_id,
                plugin.digest,
                plugin.capabilities_json,
                plugin.enabled,
                plugin.trusted_at_ms
            ],
        )?;
        Ok(())
    }

    /// Returns all trusted plugins.
    pub fn list_trusted_plugins(&self) -> StoreResult<Vec<TrustedPluginRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, name, version, key_id, digest, capabilities_json, enabled, trusted_at_ms
             FROM trusted_plugins ORDER BY trusted_at_ms DESC",
        )?;
        let rows = stmt.query_map([], TrustedPluginRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Disables (but does not delete) a trusted plugin by its manifest id.
    pub fn disable_trusted_plugin(&self, id: &str) -> StoreResult<usize> {
        let n = self.connection.execute(
            "UPDATE trusted_plugins SET enabled = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(n)
    }

    /// Permanently removes a plugin from the trust registry.
    pub fn revoke_trusted_plugin(&self, id: &str) -> StoreResult<usize> {
        let n = self
            .connection
            .execute("DELETE FROM trusted_plugins WHERE id = ?1", params![id])?;
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Calibration samples (v6)
    // -----------------------------------------------------------------------

    /// Inserts one operator-confirmed calibration sample.
    pub fn insert_calibration_sample(&self, sample: &CalibrationSampleRecord) -> StoreResult<i64> {
        self.connection.execute(
            "INSERT INTO calibration_samples
               (language, transcript_text, expected_ref, outcome, detected_ref, recorded_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                sample.language,
                sample.transcript_text,
                sample.expected_ref,
                sample.outcome,
                sample.detected_ref,
                sample.recorded_at_ms
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Returns all calibration samples, newest first.
    pub fn list_calibration_samples(&self) -> StoreResult<Vec<CalibrationSampleRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, language, transcript_text, expected_ref, outcome, detected_ref, recorded_at_ms
             FROM calibration_samples ORDER BY recorded_at_ms DESC",
        )?;
        let rows = stmt.query_map([], CalibrationSampleRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Returns aggregate accuracy metrics from stored calibration samples.
    /// Returns `(confirmed, corrected, rejected, total)`.
    pub fn calibration_summary(&self) -> StoreResult<(i64, i64, i64, i64)> {
        let confirmed: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM calibration_samples WHERE outcome = 'confirmed'",
            [],
            |row| row.get(0),
        )?;
        let corrected: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM calibration_samples WHERE outcome = 'corrected'",
            [],
            |row| row.get(0),
        )?;
        let rejected: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM calibration_samples WHERE outcome = 'rejected'",
            [],
            |row| row.get(0),
        )?;
        let total: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM calibration_samples", [], |row| {
                    row.get(0)
                })?;
        Ok((confirmed, corrected, rejected, total))
    }

    /// Persists a JSON-serialised runtime state blob to the singleton
    /// `runtime_state` row. Called on every arm/disarm, preview, live, and
    /// clear event so that a mid-service crash followed by a restart puts
    /// the operator back exactly where they were.
    pub fn save_runtime_state(&self, json: &str) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO runtime_state (id, state_json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET state_json = excluded.state_json",
            params![json],
        )?;
        Ok(())
    }

    /// Loads the persisted runtime state JSON, if any. Returns `None` on
    /// a fresh install or after a schema reset.
    pub fn load_runtime_state(&self) -> StoreResult<Option<String>> {
        Ok(self.connection.query_row(
            "SELECT state_json FROM runtime_state WHERE id = 1",
            [],
            |row| row.get(0),
        ).optional()?)
    }

    // -----------------------------------------------------------------------
    // Service sessions (v9 wiring)
    // -----------------------------------------------------------------------

    /// Creates or updates a service session row. Idempotent on `id`.
    pub fn upsert_service_session(&self, session: &ServiceSessionRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO service_sessions
               (id, name, started_at_ms, ended_at_ms, data_miser_enabled, offline_mode_enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               started_at_ms = excluded.started_at_ms,
               ended_at_ms = excluded.ended_at_ms,
               data_miser_enabled = excluded.data_miser_enabled,
               offline_mode_enabled = excluded.offline_mode_enabled",
            params![
                session.id,
                session.name,
                session.started_at_ms,
                session.ended_at_ms,
                session.data_miser_enabled as i64,
                session.offline_mode_enabled as i64,
            ],
        )?;
        Ok(())
    }

    /// Marks a session as ended.
    pub fn end_service_session(&self, id: &str, ended_at_ms: TimestampMs) -> StoreResult<()> {
        self.connection.execute(
            "UPDATE service_sessions SET ended_at_ms = ?1 WHERE id = ?2",
            params![ended_at_ms, id],
        )?;
        Ok(())
    }

    /// Returns a single service session by id.
    pub fn get_service_session(&self, id: &str) -> StoreResult<Option<ServiceSessionRecord>> {
        self.connection
            .query_row(
                "SELECT id, name, started_at_ms, ended_at_ms, data_miser_enabled, offline_mode_enabled
                 FROM service_sessions WHERE id = ?1",
                params![id],
                ServiceSessionRecord::from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Transcript segments (v9 wiring)
    // -----------------------------------------------------------------------

    /// Inserts one transcript segment row.
    pub fn insert_transcript_segment(
        &self,
        segment: &TranscriptSegmentRecord,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO transcript_segments
               (id, session_id, started_at_ms, ended_at_ms, speaker_label,
                language, text, confidence, adapter, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               started_at_ms = excluded.started_at_ms,
               ended_at_ms = excluded.ended_at_ms,
               speaker_label = excluded.speaker_label,
               language = excluded.language,
               text = excluded.text,
               confidence = excluded.confidence,
               adapter = excluded.adapter,
               latency_ms = excluded.latency_ms",
            params![
                segment.id,
                segment.session_id,
                segment.started_at_ms,
                segment.ended_at_ms,
                segment.speaker_label,
                segment.language,
                segment.text,
                segment.confidence,
                segment.adapter,
                segment.latency_ms,
            ],
        )?;
        Ok(())
    }

    /// Returns the most recent transcript segments for a session, newest first.
    pub fn recent_transcript_segments(
        &self,
        session_id: &str,
        limit: u16,
    ) -> StoreResult<Vec<TranscriptSegmentRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, session_id, started_at_ms, ended_at_ms, speaker_label,
                    language, text, confidence, adapter, latency_ms
             FROM transcript_segments
             WHERE session_id = ?1
             ORDER BY started_at_ms DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], TranscriptSegmentRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Scripture candidates (v9 wiring)
    // -----------------------------------------------------------------------

    /// Inserts one scripture candidate row.
    pub fn insert_scripture_candidate(
        &self,
        candidate: &ScriptureCandidateRecord,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO scripture_candidates
               (id, session_id, reference, translation_id, language, score,
                bucket, status, reason, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               reference = excluded.reference,
               translation_id = excluded.translation_id,
               language = excluded.language,
               score = excluded.score,
               bucket = excluded.bucket,
               status = excluded.status,
               reason = excluded.reason",
            params![
                candidate.id,
                candidate.session_id,
                candidate.reference,
                candidate.translation_id,
                candidate.language,
                candidate.score,
                candidate.bucket,
                candidate.status,
                candidate.reason,
                candidate.created_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Updates the status field of a candidate (e.g. `"pending"` →
    /// `"approved"` | `"rejected"` | `"live"` | `"cleared"`).
    pub fn update_scripture_candidate_status(
        &self,
        candidate_id: &str,
        status: &str,
    ) -> StoreResult<usize> {
        Ok(self.connection.execute(
            "UPDATE scripture_candidates SET status = ?1 WHERE id = ?2",
            params![status, candidate_id],
        )?)
    }

    /// Returns the most recent candidates for a session, newest first.
    pub fn recent_scripture_candidates(
        &self,
        session_id: &str,
        limit: u16,
    ) -> StoreResult<Vec<ScriptureCandidateRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, session_id, reference, translation_id, language, score,
                    bucket, status, reason, created_at_ms
             FROM scripture_candidates
             WHERE session_id = ?1
             ORDER BY created_at_ms DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], ScriptureCandidateRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Operator actions (v9)
    // -----------------------------------------------------------------------

    /// Records one operator verdict. Returns the new row id.
    pub fn insert_operator_action(&self, action: &OperatorActionRecord) -> StoreResult<i64> {
        self.connection.execute(
            "INSERT INTO operator_actions
               (session_id, candidate_id, action_type, actor, payload_json, occurred_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                action.session_id,
                action.candidate_id,
                action.action_type,
                action.actor,
                action.payload_json,
                action.occurred_at_ms,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Lists the most recent operator actions for a session, newest first.
    pub fn recent_operator_actions(
        &self,
        session_id: &str,
        limit: u16,
    ) -> StoreResult<Vec<OperatorActionRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, session_id, candidate_id, action_type, actor, payload_json, occurred_at_ms
             FROM operator_actions
             WHERE session_id = ?1
             ORDER BY occurred_at_ms DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], OperatorActionRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    // -----------------------------------------------------------------------
    // Display events (v9)
    // -----------------------------------------------------------------------

    /// Records a display event (preview, take-live, extend, or clear).
    pub fn insert_display_event(&self, event: &DisplayEventRecord) -> StoreResult<i64> {
        self.connection.execute(
            "INSERT INTO display_events
               (session_id, candidate_id, action, output_target, triggered_by,
                locked_at_ms, released_at_ms, detail_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.session_id,
                event.candidate_id,
                event.action,
                event.output_target,
                event.triggered_by,
                event.locked_at_ms,
                event.released_at_ms,
                event.detail_json,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Lists the most recent display events for a session, newest first.
    pub fn recent_display_events(
        &self,
        session_id: &str,
        limit: u16,
    ) -> StoreResult<Vec<DisplayEventRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, session_id, candidate_id, action, output_target,
                    triggered_by, locked_at_ms, released_at_ms, detail_json
             FROM display_events
             WHERE session_id = ?1
             ORDER BY locked_at_ms DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], DisplayEventRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    /// Gives advanced services controlled access to the connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    // -- v8: app_kv ---------------------------------------------------------

    /// Reads a UI-side document from the key/value store. Returns the raw
    /// JSON string so the caller can deserialize against any DTO shape.
    pub fn kv_get(&self, key: &str) -> StoreResult<Option<String>> {
        Ok(self.connection.query_row(
            "SELECT value_json FROM app_kv WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ).optional()?)
    }

    /// Upserts a UI-side document. `now_ms` is supplied by the caller so
    /// the store stays free of clock dependencies (testability).
    pub fn kv_set(&self, key: &str, value_json: &str, now_ms: i64) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO app_kv (key, value_json, updated_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                            updated_at_ms = excluded.updated_at_ms",
            params![key, value_json, now_ms],
        )?;
        Ok(())
    }

    /// Removes a key/value pair. Returns the number of rows deleted (0 or 1).
    pub fn kv_delete(&self, key: &str) -> StoreResult<usize> {
        Ok(self.connection.execute("DELETE FROM app_kv WHERE key = ?1", params![key])?)
    }

    /// Lists all `(key, updated_at_ms)` pairs for client-side cache headers.
    pub fn kv_list_keys(&self) -> StoreResult<Vec<(String, i64)>> {
        let mut stmt = self.connection.prepare(
            "SELECT key, updated_at_ms FROM app_kv ORDER BY key ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // -----------------------------------------------------------------------
    // Extended session / audit helpers (v9+)
    // -----------------------------------------------------------------------

    /// Alias for `get_service_session` — preferred name in new command code.
    pub fn find_service_session(&self, id: &str) -> StoreResult<Option<ServiceSessionRecord>> {
        self.get_service_session(id)
    }

    /// Returns the number of transcript segments persisted for a session.
    pub fn count_transcript_segments_for_session(&self, session_id: &str) -> StoreResult<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM transcript_segments WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?)
    }

    /// Returns the number of scripture candidates recorded for a session.
    pub fn count_scripture_candidates_for_session(&self, session_id: &str) -> StoreResult<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM scripture_candidates WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?)
    }

    /// Returns ALL operator actions for a session in chronological order.
    /// Used by the service report generator — no pagination needed here
    /// because a service produces at most a few hundred actions.
    pub fn list_operator_actions_for_session(
        &self,
        session_id: &str,
    ) -> StoreResult<Vec<OperatorActionRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, session_id, candidate_id, action_type, actor, payload_json, occurred_at_ms
             FROM operator_actions
             WHERE session_id = ?1
             ORDER BY occurred_at_ms ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![session_id], OperatorActionRecord::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    /// Full-text search over persisted transcript segments.
    /// Falls back gracefully to a plain LIKE search when the FTS5 table does
    /// not yet exist (e.g. databases migrated before v9).
    pub fn fts_search_transcript_segments(
        &self,
        query: &str,
        limit: u16,
    ) -> StoreResult<Vec<TranscriptSegmentRecord>> {
        // Try FTS5 first (requires the `transcript_segments_fts` virtual table).
        let fts_result = self.connection.prepare(
            "SELECT ts.id, ts.session_id, ts.started_at_ms, ts.ended_at_ms,
                    ts.speaker_label, ts.language, ts.text, ts.confidence,
                    ts.adapter, ts.latency_ms
             FROM transcript_segments ts
             INNER JOIN transcript_segments_fts fts ON fts.rowid = ts.rowid
             WHERE transcript_segments_fts MATCH ?1
             ORDER BY ts.started_at_ms DESC
             LIMIT ?2",
        );
        match fts_result {
            Ok(mut stmt) => {
                let rows = stmt.query_map(params![query, limit], TranscriptSegmentRecord::from_row)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
            }
            Err(_) => {
                // FTS table absent — fall back to substring search.
                let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
                let mut stmt = self.connection.prepare(
                    "SELECT id, session_id, started_at_ms, ended_at_ms, speaker_label,
                            language, text, confidence, adapter, latency_ms
                     FROM transcript_segments
                     WHERE text LIKE ?1 ESCAPE '\\'
                     ORDER BY started_at_ms DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![pattern, limit], TranscriptSegmentRecord::from_row)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
            }
        }
    }

    /// Upserts the installation state of an offline asset (STT model, Bible pack, etc.).
    pub fn upsert_offline_asset_state(&self, record: &OfflineAssetStateRecord) -> StoreResult<()> {
        self.update_offline_asset_state(record)
    }
}

/// Translation metadata with licensing state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranslationRecord {
    pub id: String,
    pub name: String,
    pub language: String,
    pub license: String,
    pub offline_ready: bool,
}

/// One scripture verse row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerseRecord {
    pub translation_id: String,
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

impl VerseRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            translation_id: row.get(0)?,
            book: row.get(1)?,
            chapter: row.get(2)?,
            verse: row.get(3)?,
            text: row.get(4)?,
        })
    }
}

/// Offline asset state row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfflineAssetStateRecord {
    pub id: String,
    pub state: String,
    pub checksum: String,
    pub updated_at_ms: TimestampMs,
}

impl OfflineAssetStateRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            state: row.get(1)?,
            checksum: row.get(2)?,
            updated_at_ms: row.get(3)?,
        })
    }
}

/// Device acceptance receipt row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceAcceptanceReceiptRecord {
    pub device_id: String,
    pub step_label: String,
    pub passed: bool,
    pub note: Option<String>,
    pub evidence_path: Option<String>,
    pub recorded_at_ms: TimestampMs,
}

impl DeviceAcceptanceReceiptRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            device_id: row.get(0)?,
            step_label: row.get(1)?,
            passed: row.get::<_, i64>(2)? != 0,
            note: row.get(3)?,
            evidence_path: row.get(4)?,
            recorded_at_ms: row.get(5)?,
        })
    }
}

/// Non-secret integration configuration row. Secrets are stored by reference only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationConfigRecord {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub enabled: bool,
    pub config_json: String,
    pub secret_ref: Option<String>,
}

impl IntegrationConfigRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            kind: row.get(1)?,
            display_name: row.get(2)?,
            enabled: row.get(3)?,
            config_json: row.get(4)?,
            secret_ref: row.get(5)?,
        })
    }
}

/// Integration delivery, health, or operator action event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationEventRecord {
    pub timestamp_ms: TimestampMs,
    pub integration_id: String,
    pub severity: String,
    pub action: String,
    pub detail: String,
    pub receipt_json: String,
}

impl IntegrationEventRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            timestamp_ms: row.get(0)?,
            integration_id: row.get(1)?,
            severity: row.get(2)?,
            action: row.get(3)?,
            detail: row.get(4)?,
            receipt_json: row.get(5)?,
        })
    }
}

/// Service profile row. Captures the operator-configured session identity,
/// language set, and live-output policy so it can be restored between services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceProfileRecord {
    /// Stable slug id (e.g. `sunday-am`).
    pub id: String,
    /// Human display name (e.g. `Sunday AM`).
    pub name: String,
    /// JSON array of BCP-47 language codes (e.g. `["en","yo","ig"]`).
    pub languages_json: String,
    /// Operator approval policy: `"manual-live"` | `"auto-preview"`.
    pub output_policy: String,
    /// Whether this is the currently selected profile.
    pub is_active: bool,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
}

impl ServiceProfileRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            languages_json: row.get(2)?,
            output_policy: row.get(3)?,
            is_active: row.get::<_, i64>(4)? != 0,
            created_at_ms: row.get(5)?,
            updated_at_ms: row.get(6)?,
        })
    }
}

/// Trusted plugin registry entry (v6).
///
/// A plugin is trust-listed only after Ed25519 signature verification passes.
/// `enabled` is a soft-delete flag; setting it to `false` blocks dispatch
/// without losing the audit trail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedPluginRecord {
    /// Stable manifest identifier (e.g. `com.aletheia.plugin.obs-gateway`).
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// SemVer string from the manifest.
    pub version: String,
    /// Fingerprint of the Ed25519 signing key.
    pub key_id: String,
    /// Hex-encoded SHA-256 digest of the canonical manifest payload.
    pub digest: String,
    /// JSON array of capability strings granted by the operator.
    pub capabilities_json: String,
    /// `false` means the plugin is disabled without being revoked.
    pub enabled: bool,
    /// Unix-epoch milliseconds when the plugin was first trusted.
    pub trusted_at_ms: TimestampMs,
}

impl TrustedPluginRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            version: row.get(2)?,
            key_id: row.get(3)?,
            digest: row.get(4)?,
            capabilities_json: row.get(5)?,
            enabled: row.get::<_, i64>(6)? != 0,
            trusted_at_ms: row.get(7)?,
        })
    }
}

/// Operator-confirmed calibration sample (v6).
///
/// Each sample captures one transcript snippet, the reference the detector
/// produced, and how the operator responded. Aggregating these rows drives the
/// accuracy metrics displayed in the Health screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalibrationSampleRecord {
    /// Auto-assigned row id. `0` on insert (filled by SQLite AUTOINCREMENT).
    pub id: i64,
    /// BCP-47 language code of the audio source.
    pub language: String,
    /// Raw transcript text that triggered the detection.
    pub transcript_text: String,
    /// Operator-supplied correct reference, or `None` for a true negative.
    pub expected_ref: Option<String>,
    /// Operator verdict: `"confirmed"` | `"corrected"` | `"rejected"`.
    pub outcome: String,
    /// Reference the detector produced (may differ from `expected_ref`).
    pub detected_ref: Option<String>,
    /// Unix-epoch milliseconds when the operator submitted the verdict.
    pub recorded_at_ms: TimestampMs,
}

impl CalibrationSampleRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            language: row.get(1)?,
            transcript_text: row.get(2)?,
            expected_ref: row.get(3)?,
            outcome: row.get(4)?,
            detected_ref: row.get(5)?,
            recorded_at_ms: row.get(6)?,
        })
    }
}

/// Service session row (v9 wiring).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceSessionRecord {
    pub id: String,
    pub name: String,
    pub started_at_ms: TimestampMs,
    pub ended_at_ms: Option<TimestampMs>,
    pub data_miser_enabled: bool,
    pub offline_mode_enabled: bool,
}

impl ServiceSessionRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            started_at_ms: row.get(2)?,
            ended_at_ms: row.get(3)?,
            data_miser_enabled: row.get::<_, i64>(4)? != 0,
            offline_mode_enabled: row.get::<_, i64>(5)? != 0,
        })
    }
}

/// Transcript segment row (v9 wiring).
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSegmentRecord {
    pub id: String,
    pub session_id: String,
    pub started_at_ms: TimestampMs,
    pub ended_at_ms: TimestampMs,
    pub speaker_label: Option<String>,
    pub language: String,
    pub text: String,
    pub confidence: f64,
    pub adapter: String,
    pub latency_ms: i64,
}

impl TranscriptSegmentRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            started_at_ms: row.get(2)?,
            ended_at_ms: row.get(3)?,
            speaker_label: row.get(4)?,
            language: row.get(5)?,
            text: row.get(6)?,
            confidence: row.get(7)?,
            adapter: row.get(8)?,
            latency_ms: row.get(9)?,
        })
    }
}

/// Scripture candidate row (v9 wiring).
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptureCandidateRecord {
    pub id: String,
    pub session_id: String,
    pub reference: String,
    pub translation_id: String,
    pub language: String,
    pub score: f64,
    pub bucket: String,
    /// `"pending"` | `"preview"` | `"live"` | `"rejected"` | `"cleared"`.
    pub status: String,
    pub reason: String,
    pub created_at_ms: TimestampMs,
}

impl ScriptureCandidateRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            reference: row.get(2)?,
            translation_id: row.get(3)?,
            language: row.get(4)?,
            score: row.get(5)?,
            bucket: row.get(6)?,
            status: row.get(7)?,
            reason: row.get(8)?,
            created_at_ms: row.get(9)?,
        })
    }
}

/// Operator action row (v9).
#[derive(Clone, Debug, PartialEq)]
pub struct OperatorActionRecord {
    /// Row id, 0 on insert.
    pub id: i64,
    pub session_id: String,
    pub candidate_id: Option<String>,
    /// `"approve"` | `"reject"` | `"preview"` | `"live"` | `"merge"` | `"extend"` | `"clear"` | `"panic_clear"`.
    pub action_type: String,
    pub actor: String,
    pub payload_json: String,
    pub occurred_at_ms: TimestampMs,
}

impl OperatorActionRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            candidate_id: row.get(2)?,
            action_type: row.get(3)?,
            actor: row.get(4)?,
            payload_json: row.get(5)?,
            occurred_at_ms: row.get(6)?,
        })
    }
}

/// Display event row (v9).
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayEventRecord {
    pub id: i64,
    pub session_id: String,
    pub candidate_id: Option<String>,
    /// `"preview"` | `"live"` | `"extend"` | `"clear"` | `"panic_clear"`.
    pub action: String,
    /// `"projector"` | `"vmix"` | `"obs"` | `"propresenter"` | `"easyworship"` | `"all"` | ...
    pub output_target: String,
    pub triggered_by: String,
    pub locked_at_ms: TimestampMs,
    pub released_at_ms: Option<TimestampMs>,
    pub detail_json: String,
}

impl DisplayEventRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            session_id: row.get(1)?,
            candidate_id: row.get(2)?,
            action: row.get(3)?,
            output_target: row.get(4)?,
            triggered_by: row.get(5)?,
            locked_at_ms: row.get(6)?,
            released_at_ms: row.get(7)?,
            detail_json: row.get(8)?,
        })
    }
}

/// Audit event row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEventRecord {
    pub timestamp_ms: TimestampMs,
    pub action: String,
    pub actor: String,
    pub detail: String,
    pub previous_hash: String,
    pub event_hash: String,
}

impl AuditEventRecord {
    /// Creates an audit row from a core action.
    pub fn from_action(
        timestamp_ms: TimestampMs,
        action: AuditAction,
        actor: impl Into<String>,
        detail: impl Into<String>,
        previous_hash: impl Into<String>,
        event_hash: impl Into<String>,
    ) -> Self {
        Self {
            timestamp_ms,
            action: format!("{action:?}"),
            actor: actor.into(),
            detail: detail.into(),
            previous_hash: previous_hash.into(),
            event_hash: event_hash.into(),
        }
    }
}

/// Store result type.
pub type StoreResult<T> = Result<T, StoreError>;

/// Store errors.
#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// The on-disk schema was written by a newer Aletheia binary.
    /// Opening it would risk silent data corruption.
    SchemaTooNew {
        on_disk: i64,
        expected: i64,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => write!(formatter, "sqlite error: {error}"),
            Self::SchemaTooNew { on_disk, expected } => write!(
                formatter,
                "database schema v{on_disk} was created by a newer Aletheia binary \
                 (this binary supports up to v{expected}); please upgrade Aletheia"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS service_sessions (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER,
  data_miser_enabled INTEGER NOT NULL DEFAULT 0,
  offline_mode_enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS translations (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  language TEXT NOT NULL,
  license TEXT NOT NULL,
  offline_ready INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS scripture_verses (
  translation_id TEXT NOT NULL REFERENCES translations(id) ON DELETE CASCADE,
  book TEXT NOT NULL,
  chapter INTEGER NOT NULL,
  verse INTEGER NOT NULL,
  text TEXT NOT NULL,
  PRIMARY KEY (translation_id, book, chapter, verse)
);

CREATE VIRTUAL TABLE IF NOT EXISTS scripture_verses_fts USING fts5(
  translation_id UNINDEXED,
  book UNINDEXED,
  chapter UNINDEXED,
  verse UNINDEXED,
  text,
  tokenize = 'unicode61'
);

CREATE TABLE IF NOT EXISTS language_aliases (
  language TEXT NOT NULL,
  alias TEXT NOT NULL,
  canonical_book TEXT NOT NULL,
  PRIMARY KEY (language, alias)
);

CREATE TABLE IF NOT EXISTS transcript_segments (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES service_sessions(id) ON DELETE CASCADE,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER NOT NULL,
  speaker_label TEXT,
  language TEXT NOT NULL,
  text TEXT NOT NULL,
  confidence REAL NOT NULL,
  adapter TEXT NOT NULL,
  latency_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS scripture_candidates (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES service_sessions(id) ON DELETE CASCADE,
  reference TEXT NOT NULL,
  translation_id TEXT NOT NULL,
  language TEXT NOT NULL,
  score REAL NOT NULL,
  bucket TEXT NOT NULL,
  status TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS presentation_scenes (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES service_sessions(id) ON DELETE CASCADE,
  reference TEXT NOT NULL,
  translation_id TEXT NOT NULL,
  theme_id TEXT NOT NULL,
  scene_json TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS integration_configs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  config_json TEXT NOT NULL,
  secret_ref TEXT
);

CREATE TABLE IF NOT EXISTS integration_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp_ms INTEGER NOT NULL,
  integration_id TEXT NOT NULL,
  severity TEXT NOT NULL,
  action TEXT NOT NULL,
  detail TEXT NOT NULL,
  receipt_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS domain_events (
  sequence INTEGER PRIMARY KEY,
  timestamp_ms INTEGER NOT NULL,
  event_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp_ms INTEGER NOT NULL,
  action TEXT NOT NULL,
  actor TEXT NOT NULL,
  detail TEXT NOT NULL,
  previous_hash TEXT NOT NULL,
  event_hash TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  event_type TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_error TEXT
);

CREATE TABLE IF NOT EXISTS offline_assets (
  id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  checksum TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS device_acceptance_receipts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  device_id TEXT NOT NULL,
  step_label TEXT NOT NULL,
  passed INTEGER NOT NULL,
  note TEXT,
  evidence_path TEXT,
  recorded_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS service_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  languages_json TEXT NOT NULL DEFAULT '["en"]',
  output_policy TEXT NOT NULL DEFAULT 'manual-live',
  is_active INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_transcript_segments_session_time
  ON transcript_segments(session_id, started_at_ms);

CREATE INDEX IF NOT EXISTS idx_scripture_candidates_session_status
  ON scripture_candidates(session_id, status, score DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp
  ON audit_log(timestamp_ms);

CREATE INDEX IF NOT EXISTS idx_integration_events_integration_time
  ON integration_events(integration_id, timestamp_ms DESC);

CREATE INDEX IF NOT EXISTS idx_device_acceptance_device_time
  ON device_acceptance_receipts(device_id, recorded_at_ms DESC);

-- v6: plugin trust registry —————————————————————————————————————————————————
CREATE TABLE IF NOT EXISTS trusted_plugins (
  id         TEXT PRIMARY KEY,        -- manifest payload id
  name       TEXT NOT NULL,
  version    TEXT NOT NULL,
  key_id     TEXT NOT NULL,           -- signing key fingerprint
  digest     TEXT NOT NULL,           -- SHA-256 of the canonical payload
  capabilities_json TEXT NOT NULL,   -- serialised capability list
  enabled    INTEGER NOT NULL DEFAULT 1,
  trusted_at_ms INTEGER NOT NULL
);

-- v6: calibration sample registry ————————————————————————————————————————————
-- Captures operator-confirmed reference assignments for ongoing accuracy work.
CREATE TABLE IF NOT EXISTS calibration_samples (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  language        TEXT NOT NULL,
  transcript_text TEXT NOT NULL,
  expected_ref    TEXT,               -- NULL means "negative" (no scripture)
  outcome         TEXT NOT NULL,      -- 'confirmed' | 'corrected' | 'rejected'
  detected_ref    TEXT,               -- what the detector said (may be NULL)
  recorded_at_ms  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trusted_plugins_key
  ON trusted_plugins(key_id);

CREATE INDEX IF NOT EXISTS idx_calibration_samples_lang
  ON calibration_samples(language, recorded_at_ms DESC);

-- v7: runtime state persistence —————————————————————————————————————————————
CREATE TABLE IF NOT EXISTS runtime_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  state_json TEXT NOT NULL
);

-- v8: generic key/value store for UI-side persistence (service plan, song
-- library, stream overlay, clip EDL). One row per logical document, namespaced
-- by `key`. Values are opaque JSON so the schema stays stable as the UI
-- evolves. updated_at_ms enables mtime-based sync if we ever need it.
CREATE TABLE IF NOT EXISTS app_kv (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

-- v9: operator actions on scripture candidates (approve, reject, merge,
-- extend, pin). Every verdict the operator records becomes one row so that
-- the queue is reconstructible after a mid-service crash and accuracy
-- analytics can be computed post-service.
CREATE TABLE IF NOT EXISTS operator_actions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES service_sessions(id) ON DELETE CASCADE,
  candidate_id TEXT,
  action_type TEXT NOT NULL,
  actor TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  occurred_at_ms INTEGER NOT NULL
);

-- v9: display events — every push to preview, take-live, extend, or clear
-- on any output target. Lets us replay a service end-to-end and compute
-- TTDisplay latency metrics.
CREATE TABLE IF NOT EXISTS display_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES service_sessions(id) ON DELETE CASCADE,
  candidate_id TEXT,
  action TEXT NOT NULL,
  output_target TEXT NOT NULL,
  triggered_by TEXT NOT NULL,
  locked_at_ms INTEGER NOT NULL,
  released_at_ms INTEGER,
  detail_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_operator_actions_session_time
  ON operator_actions(session_id, occurred_at_ms DESC);

CREATE INDEX IF NOT EXISTS idx_display_events_session_time
  ON display_events(session_id, locked_at_ms DESC);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_set_schema_version() {
        let store = AletheiaStore::open_memory().expect("store opens");

        assert_eq!(
            store.schema_version().expect("schema version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn inserts_and_finds_verse() {
        let store = AletheiaStore::open_memory().expect("store opens");
        store
            .insert_translation(&TranslationRecord {
                id: "kjv".to_string(),
                name: "King James Version".to_string(),
                language: "English".to_string(),
                license: "public-domain".to_string(),
                offline_ready: true,
            })
            .expect("translation inserted");
        store
            .insert_verse(&VerseRecord {
                translation_id: "kjv".to_string(),
                book: "1 Samuel".to_string(),
                chapter: 17,
                verse: 45,
                text: "Then said David to the Philistine, Thou comest to me with a sword."
                    .to_string(),
            })
            .expect("verse inserted");

        let verse = store
            .find_verse("kjv", "1 Samuel", 17, 45)
            .expect("query succeeds")
            .expect("verse exists");

        assert_eq!(verse.book, "1 Samuel");
        assert!(verse.text.contains("David"));
    }

    #[test]
    fn phrase_search_uses_fts() {
        let store = AletheiaStore::open_memory().expect("store opens");
        store
            .insert_translation(&TranslationRecord {
                id: "kjv".to_string(),
                name: "King James Version".to_string(),
                language: "English".to_string(),
                license: "public-domain".to_string(),
                offline_ready: true,
            })
            .expect("translation inserted");
        store
            .insert_verse(&VerseRecord {
                translation_id: "kjv".to_string(),
                book: "Psalm".to_string(),
                chapter: 23,
                verse: 4,
                text: "Yea, though I walk through the valley of the shadow of death.".to_string(),
            })
            .expect("verse inserted");

        let results = store
            .search_phrase("\"shadow of death\"", 10)
            .expect("search succeeds");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].book, "Psalm");
    }

    #[test]
    fn records_audit_event() {
        let store = AletheiaStore::open_memory().expect("store opens");
        store
            .insert_audit_event(&AuditEventRecord::from_action(
                1_700_000,
                AuditAction::LiveOutputSent,
                "operator:mary",
                "Sent Psalm 23:4 to OBS",
                "prev",
                "next",
            ))
            .expect("audit event inserted");

        let count: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
            .expect("count row");

        assert_eq!(count, 1);
    }

    #[test]
    fn upserts_and_loads_integration_config() {
        let store = AletheiaStore::open_memory().expect("store opens");
        let config = IntegrationConfigRecord {
            id: "vmix-main".to_string(),
            kind: "vmix".to_string(),
            display_name: "vMix".to_string(),
            enabled: true,
            config_json: r#"{"host":"127.0.0.1","port":8088}"#.to_string(),
            secret_ref: None,
        };

        store
            .upsert_integration_config(&config)
            .expect("config inserted");
        let loaded = store
            .get_integration_config("vmix-main")
            .expect("query succeeds")
            .expect("config exists");

        assert_eq!(loaded, config);
    }

    #[test]
    fn upserts_and_lists_service_profiles() {
        let store = AletheiaStore::open_memory().expect("store opens");
        let now = 1_700_000_u64;

        let profile = ServiceProfileRecord {
            id: "sunday-am".to_string(),
            name: "Sunday AM".to_string(),
            languages_json: r#"["en","yo","ig"]"#.to_string(),
            output_policy: "manual-live".to_string(),
            is_active: true,
            created_at_ms: now,
            updated_at_ms: now,
        };

        store
            .upsert_service_profile(&profile)
            .expect("profile saved");

        let loaded = store
            .get_active_service_profile()
            .expect("query succeeds")
            .expect("active profile exists");

        assert_eq!(loaded.id, "sunday-am");
        assert_eq!(loaded.name, "Sunday AM");
        assert!(loaded.is_active);
        assert!(loaded.languages_json.contains("\"yo\""));

        let list = store.list_service_profiles().expect("list succeeds");
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn set_active_deactivates_others() {
        let store = AletheiaStore::open_memory().expect("store opens");
        let now = 1_700_000_u64;

        for (id, name) in [("sunday-am", "Sunday AM"), ("wednesday-pm", "Wednesday PM")] {
            store
                .upsert_service_profile(&ServiceProfileRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    languages_json: r#"["en"]"#.to_string(),
                    output_policy: "manual-live".to_string(),
                    is_active: id == "sunday-am",
                    created_at_ms: now,
                    updated_at_ms: now,
                })
                .expect("profile saved");
        }

        store
            .set_active_service_profile("wednesday-pm")
            .expect("activation succeeds");

        let active = store
            .get_active_service_profile()
            .expect("query succeeds")
            .expect("active profile exists");
        assert_eq!(active.id, "wednesday-pm");

        // sunday-am should now be inactive.
        let all = store.list_service_profiles().expect("list succeeds");
        let sunday = all.iter().find(|p| p.id == "sunday-am").expect("exists");
        assert!(!sunday.is_active);
    }

    #[test]
    fn delete_inactive_profile_succeeds() {
        let store = AletheiaStore::open_memory().expect("store opens");
        let now = 1_700_000_u64;

        store
            .upsert_service_profile(&ServiceProfileRecord {
                id: "old-profile".to_string(),
                name: "Old".to_string(),
                languages_json: r#"["en"]"#.to_string(),
                output_policy: "manual-live".to_string(),
                is_active: false,
                created_at_ms: now,
                updated_at_ms: now,
            })
            .expect("profile saved");

        let deleted = store
            .delete_service_profile("old-profile")
            .expect("delete ok");
        assert_eq!(deleted, 1);

        let all = store.list_service_profiles().expect("list succeeds");
        assert!(all.is_empty());
    }

    #[test]
    fn delete_active_profile_is_a_no_op() {
        let store = AletheiaStore::open_memory().expect("store opens");
        let now = 1_700_000_u64;

        store
            .upsert_service_profile(&ServiceProfileRecord {
                id: "active-profile".to_string(),
                name: "Active".to_string(),
                languages_json: r#"["en"]"#.to_string(),
                output_policy: "manual-live".to_string(),
                is_active: true,
                created_at_ms: now,
                updated_at_ms: now,
            })
            .expect("profile saved");

        let deleted = store
            .delete_service_profile("active-profile")
            .expect("delete ok");
        assert_eq!(deleted, 0, "active profiles cannot be deleted");
    }

    #[test]
    fn records_recent_integration_events_newest_first() {
        let store = AletheiaStore::open_memory().expect("store opens");
        store
            .insert_integration_event(&IntegrationEventRecord {
                timestamp_ms: 10,
                integration_id: "vmix-main".to_string(),
                severity: "info".to_string(),
                action: "preview".to_string(),
                detail: "Preview sent".to_string(),
                receipt_json: "{}".to_string(),
            })
            .expect("event inserted");
        store
            .insert_integration_event(&IntegrationEventRecord {
                timestamp_ms: 20,
                integration_id: "vmix-main".to_string(),
                severity: "info".to_string(),
                action: "live".to_string(),
                detail: "Live sent".to_string(),
                receipt_json: "{}".to_string(),
            })
            .expect("event inserted");

        let events = store.recent_integration_events(2).expect("events load");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, "live");
        assert_eq!(events[1].action, "preview");
    }
}
