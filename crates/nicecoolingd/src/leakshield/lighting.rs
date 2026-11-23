//! Edits of the RGBpx lighting: the LED strip and the two LED controllers. They go through the
//! write path like every other field.

use nicecooling_protocol::lighting::{CONTROLLER_FIELDS, LedController, LedStrip, STRIP_FIELDS};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;

use super::fields::{EditError, repeat_edits};
use super::write::FieldEdit;

/// The edits that make the LED strip equal to `strip`. Only the fields that differ get an edit.
pub fn strip_edits(
    registry: &Registry,
    report3: &Report3,
    strip: &LedStrip,
) -> Result<Vec<FieldEdit>, EditError> {
    repeat_edits(registry, report3, 0, &STRIP_FIELDS, |id| strip.raw(id))
}

/// The edits that make LED controller `index` equal to `controller`.
pub fn controller_edits(
    registry: &Registry,
    report3: &Report3,
    index: usize,
    controller: &LedController,
) -> Result<Vec<FieldEdit>, EditError> {
    repeat_edits(registry, report3, index, &CONTROLLER_FIELDS, |id| {
        controller.raw(id)
    })
}
