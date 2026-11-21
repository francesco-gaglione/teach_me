use domain::IdGenerator;
use uuid::Uuid;

/// Generates time-ordered UUID v7 identifiers.
#[derive(Debug, Default, Clone, Copy)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn new_id(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_distinct_v7_ids() {
        let g = UuidV7Generator;
        let (a, b) = (g.new_id(), g.new_id());
        assert_ne!(a, b);
        assert_eq!(a.get_version_num(), 7);
    }
}
