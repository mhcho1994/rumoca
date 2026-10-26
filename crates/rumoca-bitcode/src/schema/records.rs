//! Operations on public wire records.

use super::*;

impl SourceId {
    /// The first entry of this artifact's own source table.
    ///
    /// A parser has to put *something* in a span before it reads the item that
    /// carries one, and every such span is overwritten before it is stored.
    /// Naming it says so, where a bare `SourceId(0)` reads as a source nobody
    /// bothered to resolve. This indexes `RbcModel::sources`; it is not a
    /// `rumoca_core::SourceId` and names no file on its own.
    pub const PLACEHOLDER: SourceId = SourceId(0);
}

impl RbcSubscript {
    /// The expression this subscript evaluates, if any. `Whole` evaluates none.
    pub fn expression(self) -> Option<ExprId> {
        match self {
            Self::Index { expression } | Self::Slice { expression } => Some(expression),
            Self::Whole => None,
        }
    }
}

impl RbcFile {
    /// Reject a file that is not RBC, or is a version this build cannot read.
    pub fn check_header(&self) -> Result<(), String> {
        if self.magic != RBC_MAGIC {
            return Err(format!(
                "not a Rumoca Bitcode file: magic is {:?}, expected {RBC_MAGIC:?}",
                self.magic
            ));
        }
        if self.bitcode_version != RBC_VERSION {
            return Err(format!(
                "unsupported bitcode version {}: this build reads version {RBC_VERSION}",
                self.bitcode_version
            ));
        }
        Ok(())
    }
}
