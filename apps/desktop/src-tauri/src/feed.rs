/// Where the session's audio comes from. Implementations block until audio is ready, at the
/// pace it plays, so the listener thread can simply loop.
pub trait AudioFeed: Send {
    /// Rate of the samples `next_chunk` returns.
    fn sample_rate(&self) -> u32;

    /// The next mono chunk in [-1, 1], or `None` once the feed has ended.
    fn next_chunk(&mut self) -> Option<Vec<f32>>;
}
