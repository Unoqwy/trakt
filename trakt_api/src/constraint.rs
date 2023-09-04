use std::{collections::HashMap, time::SystemTime};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A constraint put on a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub struct Constraint {
    /// What kind of constraint is this.
    kind: ConstraintKind,
    /// When the constraint will be automatically lifted.
    /// If null, it will need to be lifted manually.
    #[cfg_attr(
        feature = "utoipa_schemas",
        serde(with = "time::serde::rfc3339::option")
    )]
    until: Option<OffsetDateTime>,
}

/// The kind of a constraint put on a server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa_schemas", derive(utoipa::ToSchema))]
pub enum ConstraintKind {
    /// Disabled. Load balancer will ignore this server.
    Disabled,
}

/// A set of constraints to apply to a server.
///
/// Each constraint has a key, allowing it to be overwritten/removed
/// independently. This is a useful pattern when different pieces of
/// an infrastructure want servers to be constrained for a different period
/// of time each without overwritting each other.
#[derive(Debug, Clone, Default)]
pub struct Constraints {
    all: Vec<(String, Constraint)>,
}

impl Constraints {
    /// Sets a constraint by key.
    ///
    /// ## Arguments
    ///
    /// * `key` - Constraint key
    /// * `constraint` - Constraint to apply. If [`None`], acts as a remove operation
    pub fn set(&mut self, key: &str, constraint: Option<Constraint>) {
        if let Some(constraint) = constraint {
            let previous = self.all.iter_mut().find(|(it_key, _)| it_key.eq(key));
            match previous {
                Some(previous) => previous.1 = constraint,
                None => self.all.push((key.to_owned(), constraint)),
            }
        } else {
            self.all.retain(|(it_key, _)| !it_key.eq(key))
        }
    }

    /// Tests if any of the constraints matches a predicate.
    ///
    /// ## Arguments
    ///
    /// * `predicate` - Predicate to check constraints against
    pub fn any<P>(&self, predicate: P) -> bool
    where
        P: Fn(&ConstraintKind) -> bool,
    {
        let now = SystemTime::now();
        self.all
            .iter()
            .filter(|(_, constraint)| match constraint.until {
                Some(until) => until.ge(&now),
                None => true,
            })
            .any(|(_, constraint)| predicate(&constraint.kind))
    }

    /// Removes constraints that have expired.
    pub fn clear_expired(&mut self) {
        let now = SystemTime::now();
        self.all.retain(|(_, constraint)| match constraint.until {
            Some(until) => until.ge(&now),
            None => true,
        });
    }

    /// Removes all constraints.
    pub fn clear_all(&mut self) {
        self.all.clear();
    }

    /// Serializes constraints into a [`HashMap`].
    pub fn serialize_to_map(&self) -> HashMap<String, Constraint> {
        let now = SystemTime::now();
        self.all
            .iter()
            .filter(|(_, constraint)| match constraint.until {
                Some(until) => until.ge(&now),
                None => true,
            })
            .cloned()
            .collect()
    }
}
