// Pure data shapes mirroring migrations/0001_initial_schema.sql — no
// query logic here (that lives in db/, which takes a pool and returns
// these types). Grouped by domain the same way SCHEMA.md's own
// comments already group the tables, not an invented split, so doc and
// code stay easy to cross-reference:
//   auth        — User, RefreshToken
//   knowledge   — Subject, CanonicalConcept, ConceptAlias, SubjectConcept, SubjectPrerequisite
//   journey     — Journey, JourneyConcept, JourneyPrerequisite, MasteryBank
//   track       — Project, StudyThread, Message
//   content     — Source, Chunk, ContentFlag
//   assessment  — Exercise, QuizAttempt, KivReviewSession
//   audit       — AuditLog
//
// Kept domain-qualified on purpose (models::track::StudyThread, not a
// blanket re-export flattening everything to models::StudyThread) —
// the qualified path is itself documentation of which area a type
// belongs to.
//
// allow(dead_code) on each: Block 2 is migrations + models existing —
// nothing queries these yet (that's the routes/business logic built in
// later blocks). Scoped per-module here rather than crate-wide so it
// doesn't hide unrelated dead code elsewhere later.
#[allow(dead_code)]
pub mod assessment;
#[allow(dead_code)]
pub mod audit;
#[allow(dead_code)]
pub mod auth;
#[allow(dead_code)]
pub mod content;
#[allow(dead_code)]
pub mod journey;
#[allow(dead_code)]
pub mod knowledge;
#[allow(dead_code)]
pub mod track;
