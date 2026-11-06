use super::Triplet;

/// A restriction of [`Triplets::each_triplet`](super::Triplets::each_triplet): `None` matches any value,
/// `Some(0)` an empty `from` or `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TripletQuery {
    pub index: Option<u32>,
    pub type_id: Option<u32>,
    pub from: Option<u32>,
    pub to: Option<u32>,
}

impl TripletQuery {
    /// Every triplet.
    pub fn any() -> Self {
        Self::default()
    }

    pub fn new(
        index: Option<u32>,
        type_id: Option<u32>,
        from: Option<u32>,
        to: Option<u32>,
    ) -> Self {
        Self {
            index,
            type_id,
            from,
            to,
        }
    }

    pub fn by_index(index: u32) -> Self {
        Self {
            index: Some(index),
            ..Self::default()
        }
    }

    pub fn by_type(type_id: u32) -> Self {
        Self {
            type_id: Some(type_id),
            ..Self::default()
        }
    }

    pub fn by_from(from: u32) -> Self {
        Self {
            from: Some(from),
            ..Self::default()
        }
    }

    pub fn by_to(to: u32) -> Self {
        Self {
            to: Some(to),
            ..Self::default()
        }
    }

    /// Whether `triplet` satisfies every part of the restriction.
    pub fn matches(&self, triplet: &Triplet) -> bool {
        self.index.is_none_or(|index| index == triplet.index)
            && self
                .type_id
                .is_none_or(|type_id| type_id == triplet.type_id)
            && self.from.is_none_or(|from| from == triplet.from)
            && self.to.is_none_or(|to| to == triplet.to)
    }
}
