//! Packed field layout and checked extent products.

/// Exact packed layout of one field projected from a record value.
///
/// Record arrays remain compact typed values in DAE-IR. Consumers that must
/// cross a scalar execution boundary use this layout to gather the selected
/// field from each record without inventing a second packing convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordFieldLayout {
    pub(super) outer_count: usize,
    pub(super) record_width: usize,
    pub(super) field_offset: usize,
    pub(super) field_width: usize,
}

impl RecordFieldLayout {
    pub const fn outer_count(self) -> usize {
        self.outer_count
    }

    pub const fn record_width(self) -> usize {
        self.record_width
    }

    pub const fn field_offset(self) -> usize {
        self.field_offset
    }

    pub const fn field_width(self) -> usize {
        self.field_width
    }
}

pub(super) fn checked_extent_product(extents: &[u32]) -> Option<usize> {
    extents
        .iter()
        .try_fold(1usize, |count, extent| count.checked_mul(*extent as usize))
}
