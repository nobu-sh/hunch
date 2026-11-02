use crate::SourceId;

const OUTPUT_PREFIX: &str = "output:";
const INPUT_PREFIX: &str = "input:";
const APP_PREFIX: &str = "app:";

/// What a Windows source id points at. The encoding never leaves this backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A render endpoint, captured through loopback.
    Output { endpoint_id: String },
    /// A capture endpoint such as a mic.
    Input { endpoint_id: String },
    /// A process and its children, captured through process loopback.
    App { process_id: u32 },
}

impl Target {
    pub fn to_source_id(&self) -> SourceId {
        match self {
            Target::Output { endpoint_id } => {
                SourceId::new(format!("{OUTPUT_PREFIX}{endpoint_id}"))
            }
            Target::Input { endpoint_id } => SourceId::new(format!("{INPUT_PREFIX}{endpoint_id}")),
            Target::App { process_id } => SourceId::new(format!("{APP_PREFIX}{process_id}")),
        }
    }

    /// `None` for ids this backend didn't make.
    pub fn parse(id: &SourceId) -> Option<Self> {
        let id = id.as_str();
        if let Some(endpoint_id) = id.strip_prefix(OUTPUT_PREFIX) {
            return Some(Target::Output {
                endpoint_id: endpoint_id.to_owned(),
            });
        }
        if let Some(endpoint_id) = id.strip_prefix(INPUT_PREFIX) {
            return Some(Target::Input {
                endpoint_id: endpoint_id.to_owned(),
            });
        }
        let process_id = id.strip_prefix(APP_PREFIX)?.parse().ok()?;
        Some(Target::App { process_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_target() {
        let targets = [
            Target::Output {
                endpoint_id: "{0.0.0.00000000}.{abc}".to_owned(),
            },
            Target::Input {
                endpoint_id: "{0.0.1.00000000}.{def}".to_owned(),
            },
            Target::App { process_id: 4242 },
        ];

        for target in targets {
            assert_eq!(Target::parse(&target.to_source_id()), Some(target));
        }
    }

    #[test]
    fn rejects_foreign_ids() {
        assert_eq!(Target::parse(&SourceId::new("speaker")), None);
        assert_eq!(Target::parse(&SourceId::new("app:chrome")), None);
    }
}
