use std::fmt;

/// Identifies a source across `list_sources` and `open`. Opaque: callers store and pass it back
/// but never parse it, since what it encodes differs per backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// A whole audio device: an output heard through loopback, or an input such as a mic.
    Device,
    /// One application, including the child processes it plays audio from.
    App,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::Device => f.write_str("device"),
            SourceKind::App => f.write_str("app"),
        }
    }
}

/// Something that can be listened to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: SourceId,
    pub kind: SourceKind,
    /// A name to show people: the device's name, or the app's executable name.
    pub name: String,
    /// The system's default device for its direction. Always false for apps.
    pub is_default: bool,
    /// The device plays audio out, so it's captured through loopback. False for inputs and apps.
    pub is_output: bool,
}
