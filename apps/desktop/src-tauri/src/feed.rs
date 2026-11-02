/// Where the session's audio comes from. Implementations block until audio is ready, at the
/// pace it plays, or for a short wait when nothing arrives, so the listener thread can simply
/// loop and still notice when it should stop.
pub trait AudioFeed: Send {
    /// Rate of the samples `next` returns.
    fn sample_rate(&self) -> u32;

    fn next(&mut self) -> FeedEvent;
}

#[derive(Debug, Clone, PartialEq)]
pub enum FeedEvent {
    /// Mono samples in [-1, 1]. May be empty when what arrived didn't complete a chunk.
    Audio(Vec<f32>),
    /// Nothing arrived in the feed's wait.
    Waiting,
    /// The feed stopped. Nothing follows it.
    Ended(FeedEnd),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedEnd {
    /// The source went away: the app closed or the device was unplugged.
    SourceGone,
    /// Capture failed partway through.
    Failed(String),
}
