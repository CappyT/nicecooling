//! The display: edits of the pages, and the preview of a page. The page fields go through the
//! write path like every other field: the verification state of each field decides if the write
//! passes. The report 8 assets (names, icons, logo) have their own write (`assets`).

use std::time::Duration;

use nicecooling_protocol::display::{PAGE_FIELDS, PAGES, Page};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::report3::Report3;

use super::fields::{EditError, repeat_edits};
use super::write::FieldEdit;

/// How long the device shows a page that `show_page` asks for. The preview of aquasuite uses 30 s.
pub const PREVIEW_SECONDS: u32 = 30;
/// aquasuite waits this long before it sends `show_page`.
pub const SHOW_PAGE_DELAY: Duration = Duration::from_millis(25);
/// aquasuite waits this long after `show_page` before it reads report 5.
pub const PREVIEW_WAIT: Duration = Duration::from_millis(200);

/// `show_page` for page `page` (0 to 9). `None` when the registry lacks the command or `page` is
/// not below `PAGES`.
pub fn show_page_command(registry: &Registry, page: u8) -> Option<Report2> {
    (usize::from(page) < PAGES)
        .then(|| registry.command("show_page"))
        .flatten()
        .map(|c| Report2::new(c.code, (u32::from(page) << 16) | (PREVIEW_SECONDS * 10)))
}

/// The edits that make page `index` equal to `page`. Only the fields that differ get an edit.
pub fn page_edits(
    registry: &Registry,
    report3: &Report3,
    index: usize,
    page: &Page,
) -> Result<Vec<FieldEdit>, EditError> {
    repeat_edits(registry, report3, index, &PAGE_FIELDS, |id| page.raw(id))
}
