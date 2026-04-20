//! Per-node runtime presentation state and patch application.

use spi::presentation::{NodeStatus, PresentationField, PresentationPatch};

/// The current resolved presentation state of one node instance.
///
/// Built by merging manifest defaults with successive [`NodePresentationUpdate`]
/// patches. Kept in the frontend `PresentationStore` and in the engine's
/// in-memory state store.
#[derive(Debug, Clone)]
pub struct Presentation {
    pub status: NodeStatus,
    /// Last-writer-wins sequence counter per field.
    pub status_seq: u64,
    pub color: Option<String>,
    pub color_seq: u64,
    pub icon: Option<String>,
    pub icon_seq: u64,
    pub message: Option<String>,
    pub message_seq: u64,
}

impl Presentation {
    /// Construct the initial presentation baseline for a node kind.
    pub fn new(
        reports_status: bool,
        default_color: Option<String>,
        default_icon: Option<String>,
    ) -> Self {
        let status = if reports_status {
            NodeStatus::Unknown
        } else {
            NodeStatus::None
        };
        Self {
            status,
            status_seq: 0,
            color: default_color,
            color_seq: 0,
            icon: default_icon,
            icon_seq: 0,
            message: None,
            message_seq: 0,
        }
    }
}

/// Apply a presentation patch using last-writer-wins per field.
///
/// Returns the updated state. The original is not mutated; callers may
/// diff old and new to detect status transitions for persistence.
pub fn apply_patch(
    base: &Presentation,
    patch: &PresentationPatch,
    clear: &[PresentationField],
    seq: u64,
) -> Presentation {
    let mut out = base.clone();

    // Apply clear first (field-local reset), then patch overwrites.
    for field in clear {
        match field {
            PresentationField::Status if seq > base.status_seq => {
                out.status = NodeStatus::None;
                out.status_seq = seq;
            }
            PresentationField::Color if seq > base.color_seq => {
                out.color = None;
                out.color_seq = seq;
            }
            PresentationField::Icon if seq > base.icon_seq => {
                out.icon = None;
                out.icon_seq = seq;
            }
            PresentationField::Message if seq > base.message_seq => {
                out.message = None;
                out.message_seq = seq;
            }
            _ => {}
        }
    }

    if let Some(status) = patch.status {
        if seq > out.status_seq {
            out.status = status;
            out.status_seq = seq;
        }
    }
    if let Some(ref color) = patch.color {
        if seq > out.color_seq {
            out.color = Some(color.clone());
            out.color_seq = seq;
        }
    }
    if let Some(ref icon) = patch.icon {
        if seq > out.icon_seq {
            out.icon = Some(icon.clone());
            out.icon_seq = seq;
        }
    }
    if let Some(ref message) = patch.message {
        if seq > out.message_seq {
            out.message = Some(message.clone());
            out.message_seq = seq;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Presentation {
        Presentation::new(true, Some("blue-500".into()), Some("activity".into()))
    }

    #[test]
    fn patch_updates_status() {
        let updated = apply_patch(
            &base(),
            &PresentationPatch {
                status: Some(NodeStatus::Warning),
                ..Default::default()
            },
            &[],
            1,
        );
        assert_eq!(updated.status, NodeStatus::Warning);
        assert_eq!(updated.status_seq, 1);
        // other fields unchanged
        assert_eq!(updated.icon.as_deref(), Some("activity"));
    }

    #[test]
    fn older_seq_ignored() {
        let mut p = base();
        p.status_seq = 5;
        p.status = NodeStatus::Error;

        let updated = apply_patch(
            &p,
            &PresentationPatch {
                status: Some(NodeStatus::Ok),
                ..Default::default()
            },
            &[],
            3, // older than current seq=5
        );
        assert_eq!(updated.status, NodeStatus::Error);
    }

    #[test]
    fn clear_removes_field() {
        let updated = apply_patch(
            &base(),
            &PresentationPatch::default(),
            &[PresentationField::Color],
            1,
        );
        assert!(updated.color.is_none());
    }
}
