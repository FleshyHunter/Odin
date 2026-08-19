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
}

pub type VoiceSessionRegistry = Arc<Mutex<HashMap<Uuid, VoiceSession>>>;
