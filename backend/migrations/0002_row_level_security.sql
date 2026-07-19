-- Transcribed from SCHEMA.md's Row Level Security [NEW] section.
--
-- 12 flat-private tables (direct user_id where the column exists;
-- indirect via journeys/study_threads for the tables that only carry
-- journey_id/thread_id, per the doc's explicit instruction to "repeat
-- per table above, adjusting the ownership path for tables that
-- reference user_id indirectly"), plus a mixed-scope pair
-- (sources/chunks) using the exact policy text given verbatim.
--
-- Every policy reads the session var defensively — current_setting(...,
-- true) returns NULL (matching zero rows) instead of raising when unset
-- — per the doc's explicit instruction, applied consistently rather
-- than the unqualified form shown once in the doc's own abbreviated
-- "Mechanism" example.
--
-- NOTE: canonical_concepts, subjects, subject_concepts,
-- subject_prerequisites, concept_aliases get NO RLS (genuinely,
-- unconditionally shared). users is also deliberately excluded — it
-- must be queryable by email before any user_id context exists to set.

-- Direct user_id ownership
ALTER TABLE mastery_bank ENABLE ROW LEVEL SECURITY;
CREATE POLICY mastery_bank_isolation ON mastery_bank
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE journeys ENABLE ROW LEVEL SECURITY;
CREATE POLICY journey_isolation ON journeys
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE study_threads ENABLE ROW LEVEL SECURITY;
CREATE POLICY study_threads_isolation ON study_threads
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE quiz_attempts ENABLE ROW LEVEL SECURITY;
CREATE POLICY quiz_attempts_isolation ON quiz_attempts
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE kiv_review_sessions ENABLE ROW LEVEL SECURITY;
CREATE POLICY kiv_review_sessions_isolation ON kiv_review_sessions
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE projects ENABLE ROW LEVEL SECURITY;
CREATE POLICY projects_isolation ON projects
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE refresh_tokens ENABLE ROW LEVEL SECURITY;
CREATE POLICY refresh_tokens_isolation ON refresh_tokens
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

ALTER TABLE content_flags ENABLE ROW LEVEL SECURITY;
CREATE POLICY content_flags_isolation ON content_flags
  USING (user_id = current_setting('app.current_user_id', true)::uuid);

-- Indirect ownership: no direct user_id column, only journey_id ->
-- journeys.user_id.
ALTER TABLE journey_concepts ENABLE ROW LEVEL SECURITY;
CREATE POLICY journey_concepts_isolation ON journey_concepts
  USING (
    journey_id IN (
      SELECT journey_id FROM journeys
      WHERE user_id = current_setting('app.current_user_id', true)::uuid
    )
  );

ALTER TABLE journey_prerequisites ENABLE ROW LEVEL SECURITY;
CREATE POLICY journey_prerequisites_isolation ON journey_prerequisites
  USING (
    journey_id IN (
      SELECT journey_id FROM journeys
      WHERE user_id = current_setting('app.current_user_id', true)::uuid
    )
  );

-- Indirect ownership: no direct user_id column, only thread_id ->
-- study_threads.user_id.
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY audit_logs_isolation ON audit_logs
  USING (
    thread_id IN (
      SELECT thread_id FROM study_threads
      WHERE user_id = current_setting('app.current_user_id', true)::uuid
    )
  );

ALTER TABLE messages ENABLE ROW LEVEL SECURITY;
CREATE POLICY messages_isolation ON messages
  USING (
    thread_id IN (
      SELECT thread_id FROM study_threads
      WHERE user_id = current_setting('app.current_user_id', true)::uuid
    )
  );

-- Mixed-scope pair (verbatim from SCHEMA.md): material_upload rows are
-- global/always visible; prompt_upload rows are private to the owning
-- journey. chunks has no direct journey_id, only source_id, so it
-- inherits sources' policy via a subquery rather than repeating the
-- ownership check directly.
ALTER TABLE sources ENABLE ROW LEVEL SECURITY;
CREATE POLICY sources_mixed_scope ON sources
  USING (
    upload_role = 'material_upload'
    OR (
      upload_role = 'prompt_upload'
      AND journey_id IN (
        SELECT journey_id FROM journeys
        WHERE user_id = current_setting('app.current_user_id', true)::uuid
      )
    )
  );

ALTER TABLE chunks ENABLE ROW LEVEL SECURITY;
CREATE POLICY chunks_mixed_scope ON chunks
  USING (
    source_id IN (SELECT source_id FROM sources)
  );
