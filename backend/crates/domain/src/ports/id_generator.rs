use uuid::Uuid;

/// Source of new identifiers. The contract requires UUID v7 (time ordered);
/// the port only promises uniqueness so fakes can be sequential.
pub trait IdGenerator: Send + Sync {
    fn new_id(&self) -> Uuid;
}
