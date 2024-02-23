
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Deletion of an entity was denied because of a non-empty relationship with a delete rule set to `Deny` .
    #[error("deletion of entity denied due to relationship delete rule")]
    RelationshipDeniedDelete,

    /// The operation would result in a relationship that has too many targets for its declared cardinality.
    ///
    /// E.g. trying to add a new target to a `ToOne` relation that already has a target.
    #[error("the operation would result in a relationship with too many targets")]
    RelationshipTooManyTargets,

    /// The operation would result in a relationship that has too few targets for its declared cardinality.
    ///
    /// E.g. trying to remove a mandatory target from a `ToOne` relation.
    #[error("the operation would result in a relationship with too few targets")]
    RelationshipTooFewTargets,
}