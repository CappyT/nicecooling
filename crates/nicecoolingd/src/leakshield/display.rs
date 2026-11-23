//! Edits of the display pages. The page fields go through the write path like every other field:
//! the verification state of each field decides if the write passes. The report 8 assets (names,
//! start image, icons) have no registry field, so no edit reaches them.

use nicecooling_protocol::display::{PAGE_FIELDS, Page};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;

use super::fields::{EditError, repeat_edits};
use super::write::FieldEdit;

/// The edits that make page `index` equal to `page`. Only the fields that differ get an edit.
pub fn page_edits(
    registry: &Registry,
    report3: &Report3,
    index: usize,
    page: &Page,
) -> Result<Vec<FieldEdit>, EditError> {
    repeat_edits(registry, report3, index, &PAGE_FIELDS, |id| page.raw(id))
}
