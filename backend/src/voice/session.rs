// In-memory registry mapping an open voice-streaming session (one per
// active recording) to the channel its chunk-transcription results get
// pushed onto. Split out from handlers.rs so state.rs can reference the
// registry's type without depending on voice's private handler
// internals — this is the only piece of state voice:: needs in
// AppState; everything else about a recording stays entirely
// request-scoped (see voice/mod.rs's own header comment). Torn down
// per-session the instant its SSE stream ends, successfully or not —
// see handlers::stream_start's own Drop guard — so this never grows
// unbounded or needs its own TTL/eviction logic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use uuid::Uuid;

pub enum VoiceStreamEvent {
    Partial(String),
    Error(String),
}

pub struct VoiceSession {
    pub user_id: Uuid,
    pub tx: mpsc::Sender<VoiceStreamEvent>,
    // deferred.md #98 — the previous tick's full windowed transcript,
    // used to diff against the next tick's result so only the genuinely
    // new words get sent (see handlers::stream_chunk). A separate inner
    // Mutex, not just a plain field: the outer registry lock is only
    // ever held briefly to look up/clone a session, never across the
    // network call to ai_service, so this needs its own lock a caller
    // can hold onto independently via a cloned Arc.
    pub last_transcript: Arc<Mutex<String>>,
}

pub type VoiceSessionRegistry = Arc<Mutex<HashMap<Uuid, VoiceSession>>>;
