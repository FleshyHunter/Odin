-- Transcribed verbatim from SCHEMA.md (Combined_Architecture_File.md
-- v4.25 — Database Schema [LOCKED]). Table order matches the doc's own
-- dependency order exactly (content_flags is deliberately placed after
-- sources/chunks/exercises, per the doc's own "MOVED HERE" note fixing
-- a real migration-order bug in an earlier draft).

-- USERS
CREATE TABLE users (
    user_id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name             TEXT NOT NULL,
    email                    TEXT NOT NULL,
    email_normalized         TEXT NOT NULL UNIQUE,
    password_hash            TEXT NOT NULL,
    email_verified           BOOLEAN NOT NULL DEFAULT TRUE,
    password_reset_token_hash TEXT,
    password_reset_expires_at TIMESTAMPTZ,
    password_reset_used_at    TIMESTAMPTZ,
    created_at               TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE refresh_tokens (
    token_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID REFERENCES users(user_id),
    token_hash   TEXT NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ DEFAULT NOW()
);

-- CANONICAL KNOWLEDGE (stable, versioned when revised)
CREATE TABLE subjects (
    subject_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title            TEXT NOT NULL,
    normalized_name  TEXT NOT NULL UNIQUE,
    description      TEXT,
    dag_version      INTEGER DEFAULT 1,
    created_at       TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE canonical_concepts (
    concept_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title        TEXT NOT NULL,
    description  TEXT,
    created_at   TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE concept_aliases (
    alias_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    concept_id  UUID REFERENCES canonical_concepts(concept_id),
    subject_id  UUID REFERENCES subjects(subject_id),
    alias       TEXT NOT NULL,
    notes       TEXT
);

CREATE TABLE subject_concepts (
    subject_concept_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id          UUID REFERENCES subjects(subject_id),
    concept_id          UUID REFERENCES canonical_concepts(concept_id),
    learning_objective  TEXT,
    difficulty_level    INTEGER CHECK(difficulty_level BETWEEN 1 AND 5),
    order_index         INTEGER,
    generated_by        TEXT,
    confidence          REAL,
    dag_version         INTEGER DEFAULT 1,
    UNIQUE(subject_id, concept_id, dag_version)
);

CREATE TABLE subject_prerequisites (
    subject_id        UUID REFERENCES subjects(subject_id),
    concept_id        UUID REFERENCES canonical_concepts(concept_id),
    prereq_concept_id UUID REFERENCES canonical_concepts(concept_id),
    strength          TEXT CHECK(strength IN ('required','recommended')),
    dag_version       INTEGER DEFAULT 1,
    PRIMARY KEY (subject_id, concept_id, prereq_concept_id, dag_version),
    CHECK (concept_id <> prereq_concept_id),
    FOREIGN KEY (subject_id, concept_id, dag_version)
      REFERENCES subject_concepts(subject_id, concept_id, dag_version),
    FOREIGN KEY (subject_id, prereq_concept_id, dag_version)
      REFERENCES subject_concepts(subject_id, concept_id, dag_version)
);

-- JOURNEYS (mutable, user-specific)
CREATE TABLE journeys (
    journey_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID REFERENCES users(user_id),
    subject_id      UUID REFERENCES subjects(subject_id),
    goal            TEXT,
    initial_level_self_report       TEXT,
    initial_level_diagnostic_result TEXT
      CHECK(initial_level_diagnostic_result IN ('confirmed','downgraded')),
    status          TEXT CHECK(status IN ('active','paused','completed')) DEFAULT 'active',
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    last_active_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE journey_concepts (
    journey_concept_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    journey_id              UUID REFERENCES journeys(journey_id),
    concept_id              UUID REFERENCES canonical_concepts(concept_id),
    status                  TEXT CHECK(status IN
                              ('locked','available','in_progress',
                               'complete','kiv','skipped')) DEFAULT 'locked',
    order_index             INTEGER,
    is_custom_ordered       BOOLEAN DEFAULT FALSE,
    advanced_correct_streak INTEGER DEFAULT 0,
    attempt_count           INTEGER DEFAULT 0,
    last_attempt_at         TIMESTAMPTZ,
    foundation_gap          BOOLEAN DEFAULT FALSE,
    kiv_flagged_at          TIMESTAMPTZ,
    UNIQUE(journey_id, concept_id)
);

CREATE TABLE journey_prerequisites (
    journey_id        UUID REFERENCES journeys(journey_id),
    concept_id        UUID REFERENCES canonical_concepts(concept_id),
    prereq_concept_id UUID REFERENCES canonical_concepts(concept_id),
    PRIMARY KEY (journey_id, concept_id, prereq_concept_id),
    CHECK (concept_id <> prereq_concept_id),
    FOREIGN KEY (journey_id, concept_id)
      REFERENCES journey_concepts(journey_id, concept_id),
    FOREIGN KEY (journey_id, prereq_concept_id)
      REFERENCES journey_concepts(journey_id, concept_id)
);

-- GLOBAL MASTERY BANK — keyed on canonical_concept_id. Global, durable only.
CREATE TABLE mastery_bank (
    user_id          UUID REFERENCES users(user_id),
    concept_id       UUID REFERENCES canonical_concepts(concept_id),
    mastery_score    REAL DEFAULT 0.0 CHECK(mastery_score BETWEEN 0.0 AND 1.0),
    is_complete      BOOLEAN DEFAULT FALSE,
    total_attempts   INTEGER DEFAULT 0,
    last_assessed_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, concept_id)
);

-- STUDY THREADS — written only on first message
CREATE TABLE projects (
    project_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID REFERENCES users(user_id),
    name         TEXT NOT NULL,
    created_at   TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE study_threads (
    thread_id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                 UUID REFERENCES users(user_id),
    journey_id              UUID REFERENCES journeys(journey_id),
    project_id              UUID REFERENCES projects(project_id),
    title                   TEXT,
    is_pinned               BOOLEAN DEFAULT FALSE,
    mode                    TEXT CHECK(mode IN ('journey','memoryless','tangent')) DEFAULT 'memoryless',
    current_concept_id      UUID REFERENCES canonical_concepts(concept_id),
    pre_tangent_concept_id  UUID REFERENCES canonical_concepts(concept_id),
    state                   JSONB,
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    last_active_at          TIMESTAMPTZ DEFAULT NOW(),
    deleted_at              TIMESTAMPTZ
);

CREATE TABLE messages (
    message_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id   UUID REFERENCES study_threads(thread_id),
    role        TEXT CHECK(role IN ('user','tutor')),
    content     TEXT NOT NULL,
    mode        TEXT CHECK(mode IN ('journey','memoryless','tangent')),
    timestamp   TIMESTAMPTZ DEFAULT NOW()
);

-- SOURCES AND CHUNKS
CREATE TABLE sources (
    source_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url             TEXT,
    filename        TEXT,
    title           TEXT,
    author_org      TEXT,
    source_type     TEXT NOT NULL CHECK(source_type IN ('user_upload','web_fetch','manual')),
    upload_role     TEXT NOT NULL CHECK(upload_role IN ('prompt_upload','material_upload')),
    upload_scope    TEXT CHECK(upload_scope IN ('journey','global')),
    journey_id      UUID REFERENCES journeys(journey_id),
    license_status  TEXT,
    trust_score     REAL NOT NULL CHECK(trust_score >= 0 AND trust_score <= 1),
    content_hash    TEXT,
    retrieval_date  TIMESTAMPTZ,
    subject_id      UUID REFERENCES subjects(subject_id),
    CHECK (
      (upload_role = 'prompt_upload' AND upload_scope = 'journey' AND journey_id IS NOT NULL)
      OR
      (upload_role = 'material_upload' AND upload_scope = 'global' AND journey_id IS NULL)
    )
);

CREATE UNIQUE INDEX sources_content_hash_global_unique
  ON sources(content_hash)
  WHERE upload_role = 'material_upload' AND content_hash IS NOT NULL;

CREATE UNIQUE INDEX sources_content_hash_journey_unique
  ON sources(journey_id, content_hash)
  WHERE upload_role = 'prompt_upload' AND content_hash IS NOT NULL;

CREATE TABLE chunks (
    chunk_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id    UUID REFERENCES sources(source_id),
    concept_id   UUID REFERENCES canonical_concepts(concept_id),
    chunk_type   TEXT CHECK(chunk_type IN ('definition','explanation','example','exercise','misc')),
    difficulty   TEXT CHECK(difficulty IN ('basic','intermediate','advanced')),
    text         TEXT NOT NULL,
    token_count  INTEGER,
    chroma_id    TEXT
);

CREATE UNIQUE INDEX chunks_chroma_id_unique
  ON chunks(chroma_id) WHERE chroma_id IS NOT NULL;

-- EXERCISES AND ATTEMPTS
CREATE TABLE exercises (
    exercise_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    concept_id      UUID REFERENCES canonical_concepts(concept_id),
    exercise_type   TEXT CHECK(exercise_type IN ('mcq','numeric','symbolic_math','fill_blank','short_answer')),
    difficulty      TEXT CHECK(difficulty IN ('basic','intermediate','advanced')),
    template_body   JSONB NOT NULL,
    template_params JSONB,
    correct_answer  TEXT,
    grader_type     TEXT,
    grader_config   JSONB,
    tolerance       REAL,
    is_canonical    BOOLEAN DEFAULT TRUE
);

CREATE UNIQUE INDEX one_canonical_template_per_concept_difficulty
  ON exercises(concept_id, difficulty)
  WHERE is_canonical = TRUE;

CREATE TABLE quiz_attempts (
    attempt_id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID REFERENCES users(user_id),
    exercise_id          UUID REFERENCES exercises(exercise_id),
    thread_id            UUID REFERENCES study_threads(thread_id),
    journey_id           UUID REFERENCES journeys(journey_id),
    rendered_question    TEXT,
    instantiated_params  JSONB,
    student_answer       TEXT,
    expected_answer       TEXT,
    is_correct           BOOLEAN,
    score                REAL,
    grader_used          TEXT,
    feedback             TEXT,
    difficulty_attempted TEXT CHECK(difficulty_attempted IN ('basic','intermediate','advanced')),
    timestamp            TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE content_flags (
    flag_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID REFERENCES users(user_id),
    source_id    UUID REFERENCES sources(source_id),
    chunk_id     UUID REFERENCES chunks(chunk_id),
    exercise_id  UUID REFERENCES exercises(exercise_id),
    reason       TEXT,
    created_at   TIMESTAMPTZ DEFAULT NOW(),
    resolved_at  TIMESTAMPTZ,
    CHECK (
      (CASE WHEN source_id IS NOT NULL THEN 1 ELSE 0 END) +
      (CASE WHEN chunk_id IS NOT NULL THEN 1 ELSE 0 END) +
      (CASE WHEN exercise_id IS NOT NULL THEN 1 ELSE 0 END) = 1
    )
);

-- KIV REVIEW SESSIONS
CREATE TABLE kiv_review_sessions (
    review_session_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            UUID REFERENCES users(user_id),
    journey_id         UUID REFERENCES journeys(journey_id),
    review_type        TEXT CHECK(review_type IN ('current_concept','mixed')),
    concept_ids        JSONB,
    triggered_at       TIMESTAMPTZ DEFAULT NOW(),
    completed_at       TIMESTAMPTZ
);

-- AUDIT LOG — mandatory every turn
CREATE TABLE audit_logs (
    log_id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id           UUID REFERENCES study_threads(thread_id),
    user_input          TEXT,
    cleaned_query       TEXT,
    matched_concepts    JSONB,
    detected_intent     TEXT,
    mode                TEXT CHECK(mode IN ('journey','memoryless','tangent')),
    retrieved_chunk_ids JSONB,
    strategy_used       TEXT,
    response_text       TEXT,
    assessment_result   JSONB,
    model_used          TEXT,
    prompt_version      TEXT,
    error               TEXT,
    queued_ms           INTEGER,
    generation_ms       INTEGER,
    timestamp           TIMESTAMPTZ DEFAULT NOW()
);
