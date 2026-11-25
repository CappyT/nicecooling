//! The display pages of the Leakshield: the images of the device, the page editor, the logo and
//! icon import, and the controls of the other tabs that use the same daemon calls.

mod support;

use nicecooling::bitmap::{Bitmap, Gray, ImportError, import_png, import_png_file};
use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{AppState, DaemonEvent, FieldValue, Link, PagePreview};
use nicecooling::screens::leakshield::display::{
    Dialog, DialogModel, DisplayModel, DisplayUi, ImportTarget, LAYOUTS, NO_ASSET, NO_TEMPLATE,
    Screen, Slots, TEMPLATES, template_slots, text_target,
};
use nicecooling::screens::leakshield::overview::{OverviewModel, OverviewUi};
use nicecooling::screens::leakshield::sensors::{SensorsModel, SensorsUi, set_scale};
use nicecooling::screens::leakshield::status::{StatusModel, StatusUi};
use nicecooling::screens::leakshield::system::{SystemModel, SystemUi, duration_text};
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fixtures::{LEAKSHIELD, NOW_US, with_leakshield};
use support::harness::Harness;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback, locale: Locale) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale,
        down_since_us: None,
        feedback,
    }
}

fn en<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    input(state, feedback, EN)
}

fn display(state: &AppState, ui: &DisplayUi) -> DisplayModel {
    DisplayModel::build(&en(state, &Feedback::default()), ui, LEAKSHIELD)
}

fn set_field(state: &mut AppState, id: &str, value: FieldValue) {
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.fields.iter_mut().find(|f| f.id == id).unwrap().value = value;
}

/// The fixture with `template` on page 1.
fn with_template(template: &str) -> AppState {
    let mut state = with_leakshield();
    let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
    templates[0] = template.to_owned();
    set_field(&mut state, "page_template", FieldValue::TextList(templates));
    state
}

fn unknown_firmware() -> AppState {
    let mut state = with_leakshield();
    state.snapshot.leakshields[0].read_only_reason = "unknown-firmware".into();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    for field in &mut data.fields {
        field.writable = false;
        field.reason = "unknown-firmware".into();
    }
    state
}

fn device_away() -> AppState {
    let mut state = with_leakshield();
    state.snapshot.leakshields[0].connected = false;
    state
}

fn daemon_offline() -> AppState {
    let mut state = with_leakshield();
    state.link = Link::Offline;
    state
}

fn preview(state: &mut AppState, page: u8, preview: PagePreview) {
    state
        .display_previews
        .insert((LEAKSHIELD.to_owned(), page), preview);
}

/// A PNG file of `width` x `height` with the color that `f` gives for each pixel.
fn png(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    use image::ImageEncoder as _;
    let picture = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(f(x, y)));
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            picture.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .expect("a PNG");
    bytes
}

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

// ---------------------------------------------------------------------------------------------
// The bitmap and its draw model.

#[test]
fn a_bitmap_has_bit_7_at_the_left_and_row_0_at_the_top() {
    // 128 x 64: 16 bytes for each row. Byte 0 is the first 8 pixels of the top row.
    let mut rows = vec![0u8; 1024];
    rows[0] = 0x80; // the top left pixel
    rows[15] = 0x01; // the top right pixel
    rows[16] = 0x40; // line 1, column 1
    rows[63 * 16] = 0x80; // the bottom left pixel
    let bitmap = Bitmap {
        width: 128,
        height: 64,
        rows,
    };
    assert!(bitmap.is_valid());
    assert!(
        bitmap.pixel(0, 0),
        "bit 7 of byte 0 is the leftmost pixel of row 0"
    );
    assert!(
        bitmap.pixel(127, 0),
        "bit 0 of byte 15 is the rightmost pixel of row 0"
    );
    assert!(bitmap.pixel(1, 1) && !bitmap.pixel(0, 1));
    assert!(bitmap.pixel(0, 63), "the last row is the bottom line");
    assert!(
        !bitmap.pixel(128, 0) && !bitmap.pixel(0, 64),
        "outside is dark"
    );
    assert_eq!(bitmap.lit_count(), 4);

    // An icon has 23 columns: 3 bytes for each row, and the last bit of a row is padding.
    let mut icon = Bitmap::new(23, 23);
    assert_eq!((icon.stride(), icon.rows.len()), (3, 69));
    icon.set(22, 0, true);
    icon.set(0, 22, true);
    assert_eq!(icon.rows[2], 0x02, "column 22 is bit 1 of the third byte");
    assert_eq!(icon.rows[22 * 3], 0x80);
    icon.set(23, 0, true);
    assert_eq!(icon.rows[2], 0x02, "a pixel outside changes nothing");
    icon.set(22, 0, false);
    assert_eq!(icon.rows[2], 0);

    // A wrong byte count is not a bitmap.
    let short = Bitmap {
        width: 128,
        height: 64,
        rows: vec![0; 1023],
    };
    assert!(!short.is_valid() && !Bitmap::default().is_valid());
}

#[test]
fn the_draw_model_is_one_run_for_each_row_of_lit_pixels() {
    let mut bitmap = Bitmap::new(128, 64);
    for x in 3..10 {
        bitmap.set(x, 0, true);
    }
    bitmap.set(12, 0, true);
    for x in 120..128 {
        bitmap.set(x, 5, true);
    }
    bitmap.set(0, 63, true);
    // (x, y, length), from the top line down and from the left.
    assert_eq!(
        bitmap.runs(),
        [(3, 0, 7), (12, 0, 1), (120, 5, 8), (0, 63, 1)]
    );
    assert!(Bitmap::new(128, 64).runs().is_empty());
    // A run crosses the byte borders of a row and ends at the last column.
    let mut full = Bitmap::new(23, 2);
    for x in 0..23 {
        full.set(x, 1, true);
    }
    assert_eq!(full.runs(), [(0, 1, 23)]);
}

// ---------------------------------------------------------------------------------------------
// The slots of each template.

#[test]
fn each_template_uses_the_slots_of_its_layout() {
    let slots = |values, texts, icons, ranges| Slots {
        values,
        texts,
        icons,
        ranges,
        logo: false,
    };
    let expected = [
        (
            "logo",
            Slots {
                logo: true,
                ..slots(0, 0, 0, 0)
            },
        ),
        ("chart", slots(1, 0, 0, 0)),
        ("values-1", slots(1, 1, 1, 0)),
        ("values-2", slots(2, 2, 2, 0)),
        ("values-4", slots(4, 4, 0, 0)),
        ("values-3a", slots(3, 1, 2, 0)),
        ("values-3b", slots(3, 1, 1, 0)),
        ("time", slots(0, 0, 0, 0)),
        ("bars-2", slots(2, 2, 2, 2)),
        ("bars-4a", slots(4, 0, 2, 4)),
        ("bars-4b", slots(4, 1, 0, 4)),
        ("value-and-bar", slots(1, 1, 0, 1)),
        ("value-and-time", slots(1, 1, 1, 0)),
        ("leakshield-state", slots(0, 0, 0, 0)),
    ];
    assert_eq!(LAYOUTS, expected);
    // The table has the templates of the registry, in their order, with the same value count.
    for ((name, slots), (template, values)) in LAYOUTS.iter().zip(TEMPLATES) {
        assert_eq!(*name, template);
        assert_eq!(slots.values, values, "{name}");
        // A page has 8 value slots and 4 icon slots. A bar has two of the 8 limits.
        assert!(
            slots.values <= 4 && slots.texts <= 4 && slots.icons <= 4,
            "{name}"
        );
        assert!(
            slots.ranges <= slots.values && slots.ranges * 2 <= 8,
            "{name}"
        );
    }
    assert_eq!(template_slots(NO_TEMPLATE), Slots::default());
    assert_eq!(template_slots("unknown-14"), Slots::default());
}

#[test]
fn the_page_card_shows_exactly_the_slots_of_the_template() {
    for (template, slots) in LAYOUTS {
        let model = display(&with_template(template), &DisplayUi::default());
        let page = &model.page;
        assert_eq!(page.rows.len(), slots.values, "{template}: values");
        assert_eq!(page.texts.len(), slots.texts, "{template}: texts");
        assert_eq!(page.icons.len(), slots.icons, "{template}: icons");
        assert_eq!(page.logo.is_some(), slots.logo, "{template}: logo");
        assert_eq!(
            page.rows.iter().filter(|row| row.range.is_some()).count(),
            slots.ranges,
            "{template}: bars"
        );
        // The limits of a bar have their columns only on a template with bars.
        assert_eq!(
            page.value_columns.len(),
            if slots.ranges > 0 { 5 } else { 3 },
            "{template}"
        );
        // Each value has the decimals 0 to 3.
        for row in &page.rows {
            assert_eq!(
                row.decimals.iter().map(|d| d.0).collect::<Vec<_>>(),
                [0, 1, 2, 3]
            );
        }
        // A template without a slot says so.
        assert_eq!(
            page.no_settings.is_some(),
            slots == Slots::default(),
            "{template}"
        );
    }
    let page = display(&with_template("bars-2"), &DisplayUi::default()).page;
    assert_eq!(
        page.value_columns,
        ["Value", "Data source", "Decimals", "Minimum", "Maximum"]
    );
    assert_eq!(page.rows[1].range, Some(("0".to_owned(), "100".to_owned())));
    assert_eq!(page.texts[1].label, "Text 2");
    assert_eq!(page.icons[1].label, "Icon 2");
    // An empty page has no slot and no note: it has "Choose a template".
    let empty = display(&with_template(NO_TEMPLATE), &DisplayUi::default()).page;
    assert!(empty.rows.is_empty() && empty.no_settings.is_none());
    assert_eq!(empty.change.label, "Choose a template");
}

// ---------------------------------------------------------------------------------------------
// The conversion of a PNG file.

#[test]
fn a_png_of_the_display_size_converts_pixel_by_pixel_at_the_threshold() {
    // The left half is white, the right half is the grey 128 (50.2 % brightness). Line 1 has
    // one black pixel at column 3 in the white half.
    let file = png(128, 64, |x, y| match (x, y) {
        (3, 1) => BLACK,
        (x, _) if x < 64 => WHITE,
        _ => [128, 128, 128, 255],
    });
    let picture = import_png(&file, 128, 64).expect("a picture");
    assert_eq!((picture.width, picture.height), (128, 64));
    assert_eq!(picture.source, (128, 64));
    assert!(!picture.scaled(), "a file of 128 x 64 is not scaled");
    assert_eq!(picture.luma.len(), 128 * 64);

    // 50 %: the grey 128 is lit. 51 %: it is dark.
    let at_50 = picture.to_bitmap(50, false);
    assert!(at_50.is_valid());
    assert_eq!(
        (at_50.width, at_50.height, at_50.rows.len()),
        (128, 64, 1024)
    );
    assert_eq!(at_50.lit_count(), 128 * 64 - 1);
    // Row 1 starts with the bits 1110 1111: column 3 is dark.
    assert_eq!(at_50.rows[16], 0xEF);
    let at_51 = picture.to_bitmap(51, false);
    assert_eq!(at_51.lit_count(), 64 * 64 - 1);
    assert!(at_51.pixel(63, 0) && !at_51.pixel(64, 0));
    // 100 %: only white. 0 %: every pixel.
    assert_eq!(picture.to_bitmap(100, false).lit_count(), 64 * 64 - 1);
    assert_eq!(picture.to_bitmap(0, false).lit_count(), 128 * 64);
    // "Swap lit and dark".
    let swapped = picture.to_bitmap(51, true);
    assert_eq!(swapped.lit_count(), 64 * 64 + 1);
    assert!(swapped.pixel(3, 1) && !swapped.pixel(0, 0));

    // A transparent pixel is dark, whatever its color.
    let clear = png(
        128,
        64,
        |x, _| if x < 10 { [255, 255, 255, 0] } else { WHITE },
    );
    let bitmap = import_png(&clear, 128, 64).unwrap().to_bitmap(50, false);
    assert!(!bitmap.pixel(9, 0) && bitmap.pixel(10, 0));
}

#[test]
fn a_png_of_another_size_is_scaled_to_fit_and_not_refused() {
    // A square of 256 x 256 becomes 64 x 64 in the middle: 32 dark columns at each side.
    let square = import_png(&png(256, 256, |_, _| WHITE), 128, 64).expect("a picture");
    assert!(square.scaled());
    assert_eq!(square.source, (256, 256));
    assert_eq!((square.width, square.height), (128, 64));
    let bitmap = square.to_bitmap(50, false);
    assert_eq!(bitmap.lit_count(), 64 * 64);
    assert!(!bitmap.pixel(31, 30) && bitmap.pixel(32, 30));
    assert!(bitmap.pixel(95, 30) && !bitmap.pixel(96, 30));

    // A wide file of 300 x 100 takes the full width and 43 lines in the middle.
    let wide = import_png(&png(300, 100, |_, _| WHITE), 128, 64).unwrap();
    let bitmap = wide.to_bitmap(50, false);
    assert_eq!(bitmap.lit_count(), 128 * 43);
    assert!(!bitmap.pixel(0, 9) && bitmap.pixel(0, 10) && bitmap.pixel(127, 52));
    assert!(!bitmap.pixel(0, 53));

    // A small file of 64 x 32 grows to the full size with hard edges: a checker of 1 pixel
    // becomes a checker of 2 pixels.
    let small = png(64, 32, |x, y| if (x + y) % 2 == 0 { WHITE } else { BLACK });
    let bitmap = import_png(&small, 128, 64).unwrap().to_bitmap(50, false);
    assert_eq!(bitmap.lit_count(), 128 * 64 / 2);
    assert!(bitmap.pixel(0, 0) && bitmap.pixel(1, 1) && !bitmap.pixel(2, 0));

    // A large picture takes the mean of its pixels: a fine black and white checker is grey, so
    // the threshold decides.
    let checker = png(
        1280,
        640,
        |x, y| if (x + y) % 2 == 0 { WHITE } else { BLACK },
    );
    let grey = import_png(&checker, 128, 64).unwrap();
    assert_eq!(grey.to_bitmap(40, false).lit_count(), 128 * 64);
    assert_eq!(grey.to_bitmap(60, false).lit_count(), 0);

    // The same path gives an icon of 23 x 23.
    let icon = import_png(
        &png(46, 46, |x, _| if x < 2 { WHITE } else { BLACK }),
        23,
        23,
    )
    .unwrap();
    assert_eq!((icon.width, icon.height, icon.scaled()), (23, 23, true));
    let bitmap = icon.to_bitmap(50, false);
    assert_eq!((bitmap.rows.len(), bitmap.lit_count()), (69, 23));
    assert!(bitmap.pixel(0, 22) && !bitmap.pixel(1, 0));
    let exact = import_png(&png(23, 23, |_, _| WHITE), 23, 23).unwrap();
    assert!(!exact.scaled());
}

#[test]
fn a_file_that_is_no_png_is_refused_with_its_reason() {
    assert_eq!(
        import_png(b"not a picture", 128, 64),
        Err(ImportError::NotPng)
    );
    assert_eq!(import_png(&[], 128, 64), Err(ImportError::NotPng));
    // The start of a JPEG file.
    assert_eq!(
        import_png(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 16], 128, 64),
        Err(ImportError::NotPng)
    );
    // A PNG file that is cut.
    let whole = png(128, 64, |_, _| WHITE);
    assert_eq!(
        import_png(&whole[..whole.len() / 2], 128, 64),
        Err(ImportError::NotPng)
    );
    let missing = std::env::temp_dir().join("nicecooling-no-such-file.png");
    assert!(matches!(
        import_png_file(&missing, 128, 64),
        Err(ImportError::Read(_))
    ));

    // The dialog names the reason and writes nothing.
    let state = with_template("logo");
    let feedback = Feedback::default();
    let mut ui = DisplayUi::default();
    ui.open_import(ImportTarget::Logo);
    ui.import_loaded("notes.txt".into(), Err(ImportError::NotPng));
    let Some(DialogModel::Import {
        error,
        result,
        write,
        file,
        ..
    }) = display(&state, &ui).dialog
    else {
        panic!("the import dialog");
    };
    assert_eq!(
        error.as_deref(),
        Some("nicecooling cannot read this file as a PNG image.")
    );
    assert_eq!(file.as_deref(), Some("notes.txt"));
    assert!(result.is_none() && !write.enabled);
    assert_eq!(write.reason.as_deref(), Some("Choose a file first."));
    assert!(
        ui.write_import(&en(&state, &feedback), LEAKSHIELD)
            .is_none()
    );
}

#[test]
fn the_import_dialog_shows_the_result_before_the_write() {
    let state = with_template("logo");
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let page = display(&state, &DisplayUi::default()).page;
    let logo = page.logo.expect("the logo row");
    assert_eq!(logo.change.label, "Change logo");
    assert!(logo.change.enabled && logo.bitmap.is_some());

    let mut ui = DisplayUi::default();
    ui.open_import(ImportTarget::Logo);
    // Without a file the dialog has no result and "Write the logo" is disabled.
    let Some(DialogModel::Import { result, write, .. }) = display(&state, &ui).dialog else {
        panic!("the import dialog");
    };
    assert!(result.is_none() && !write.enabled);

    // A file of 256 x 128: the left half is white, the right half is the grey 100.
    let file = png(
        256,
        128,
        |x, _| if x < 128 { WHITE } else { [100, 100, 100, 255] },
    );
    ui.import_loaded("logo.png".into(), import_png(&file, 128, 64));
    let Some(DialogModel::Import {
        result,
        note,
        threshold,
        threshold_label,
        write,
        title,
        ..
    }) = display(&state, &ui).dialog
    else {
        panic!("the import dialog");
    };
    assert_eq!(title, "Change the logo");
    let (bitmap, scale) = result.expect("the result");
    assert_eq!(scale, 2, "the logo shows at 2 window pixels for each pixel");
    assert_eq!(bitmap.lit_count(), 64 * 64);
    assert_eq!(threshold, 50);
    assert_eq!(threshold_label, "Lit from 50 % brightness");
    assert_eq!(
        note.as_deref(),
        Some("The file has 256 x 128 pixels. nicecooling scales it to fit 128 x 64.")
    );
    assert!(write.enabled);

    // The slider moves the threshold: at 30 % the grey 100 (39 %) is lit too.
    ui.set_threshold(30.0);
    let Some(DialogModel::Import { result, .. }) = display(&state, &ui).dialog else {
        panic!("the import dialog");
    };
    assert_eq!(result.unwrap().0.lit_count(), 128 * 64);
    ui.set_threshold(140.0);
    assert!(matches!(&ui.dialog, Some(Dialog::Import(import)) if import.threshold == 100));
    ui.set_threshold(50.0);
    ui.set_invert(true);

    // "Write the logo" sends what the dialog shows, and closes the dialog.
    let shown = match &ui.dialog {
        Some(Dialog::Import(import)) => import.result().unwrap(),
        _ => panic!("the import"),
    };
    assert_eq!(shown.lit_count(), 64 * 64, "the swapped halves");
    assert!(!shown.pixel(0, 0) && shown.pixel(127, 0));
    let command = ui
        .write_import(&screen_input, LEAKSHIELD)
        .expect("a command");
    assert_eq!(
        command,
        Command::SetDisplayLogo {
            leakshield: LEAKSHIELD.into(),
            page: 0,
            logo: shown,
        }
    );
    assert!(ui.dialog.is_none());
    assert_eq!(
        command.feedback_key().as_deref(),
        Some(format!("ls:{LEAKSHIELD}:display:page:0").as_str())
    );
    assert_eq!(command.preview_page(), Some((LEAKSHIELD, 0)));

    // The icon import replaces the icon that the slot shows. Without an icon in the slot the
    // import has no target.
    let mut state = with_template("values-1");
    let mut ui = DisplayUi::default();
    ui.open_icons(0);
    let Some(DialogModel::Icons {
        icons,
        import,
        none,
        ..
    }) = display(&state, &ui).dialog
    else {
        panic!("the icon dialog");
    };
    assert_eq!(icons.len(), 16);
    assert!(
        icons
            .iter()
            .all(|(_, icon, selected)| icon.is_valid() && !selected)
    );
    assert!(none.enabled && !import.enabled);
    assert_eq!(
        import.reason.as_deref(),
        Some("Pick an icon first. The import replaces the picked icon on every page that uses it.")
    );
    let mut icon_list = vec![255.0; 40];
    icon_list[0] = 7.0;
    set_field(&mut state, "page_icons", FieldValue::List(icon_list));
    let Some(DialogModel::Icons { icons, import, .. }) = display(&state, &ui).dialog else {
        panic!("the icon dialog");
    };
    assert!(import.enabled && icons[7].2);
    ui.open_import(ImportTarget::Icon { slot: 0 });
    ui.import_loaded(
        "drop.png".into(),
        import_png(
            &png(23, 23, |x, y| if x == y { WHITE } else { BLACK }),
            23,
            23,
        ),
    );
    let Some(DialogModel::Import {
        result,
        text,
        title,
        ..
    }) = display(&state, &ui).dialog
    else {
        panic!("the import dialog");
    };
    assert_eq!(title, "Import an icon");
    assert!(
        text.contains("23 x 23") && text.contains("icon 8"),
        "{text}"
    );
    assert_eq!(result.as_ref().map(|r| r.1), Some(4));
    let feedback = Feedback::default();
    let command = ui
        .write_import(&en(&state, &feedback), LEAKSHIELD)
        .expect("a command");
    let Command::SetDisplayIcon {
        index, icon, page, ..
    } = command
    else {
        panic!("an icon write");
    };
    assert_eq!((index, page), (7, 0));
    assert_eq!((icon.width, icon.height, icon.lit_count()), (23, 23, 23));
    assert!(icon.pixel(0, 0) && icon.pixel(22, 22) && !icon.pixel(22, 0));
}

// ---------------------------------------------------------------------------------------------
// The page editor.

fn fields_of(command: Option<Command>) -> (String, String, Vec<f64>) {
    let Some(Command::SetLeakshieldFields {
        control, fields, ..
    }) = command
    else {
        panic!("a field write");
    };
    let FieldValue::List(list) = &fields[0].1 else {
        panic!("a list");
    };
    (control, fields[0].0.clone(), list.clone())
}

#[test]
fn the_page_editor_writes_decimals_ranges_texts_and_icons() {
    let mut state = with_leakshield();
    let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
    templates[2] = "bars-2".into();
    set_field(&mut state, "page_template", FieldValue::TextList(templates));
    let feedback = Feedback::default();
    let screen_input = en(&state, &feedback);
    let mut ui = DisplayUi::default();
    ui.select(2);

    // Decimals: one entry of `page_precisions`, 8 for each page.
    let (control, field, list) = fields_of(ui.set_decimals(&screen_input, LEAKSHIELD, 1, 3));
    assert_eq!(
        (control.as_str(), field.as_str()),
        ("display:page:2", "page_precisions")
    );
    assert_eq!(list.len(), 80);
    assert_eq!(list[2 * 8 + 1], 3.0);
    assert_eq!(list.iter().filter(|v| **v != 0.0).count(), 1);
    assert!(ui.set_decimals(&screen_input, LEAKSHIELD, 0, 4).is_none());

    // The minimum and the maximum of value 2 are the entries 2 and 3 of the 8 limits.
    let (_, field, list) = fields_of(ui.set_range(&screen_input, LEAKSHIELD, 1, false, -40.4));
    assert_eq!(field, "page_values");
    assert_eq!(list[2 * 8 + 2], -40.0, "a whole number");
    let (_, _, list) = fields_of(ui.set_range(&screen_input, LEAKSHIELD, 1, true, 1500.0));
    assert_eq!(list[2 * 8 + 3], 1500.0);
    // The device takes -32000 to 32000: a larger number is cut to the limit.
    let (_, _, list) = fields_of(ui.set_range(&screen_input, LEAKSHIELD, 0, true, 99999.0));
    assert_eq!(list[2 * 8 + 1], 32000.0);
    let (_, _, list) = fields_of(ui.set_range(&screen_input, LEAKSHIELD, 0, false, -99999.0));
    assert_eq!(list[2 * 8], -32000.0);
    assert!(
        ui.set_range(&screen_input, LEAKSHIELD, 0, true, f64::NAN)
            .is_none()
    );
    assert!(
        ui.set_range(&screen_input, LEAKSHIELD, 4, true, 1.0)
            .is_none()
    );

    // A text is an index into the 40 names, or 255.
    let (_, field, list) = fields_of(ui.set_text(&screen_input, LEAKSHIELD, 1, 21));
    assert_eq!(field, "page_names");
    assert_eq!(list[2 * 8 + 1], 21.0);
    let (_, _, list) = fields_of(ui.set_text(&screen_input, LEAKSHIELD, 0, NO_ASSET));
    assert_eq!(list[2 * 8], 255.0);
    assert!(ui.set_text(&screen_input, LEAKSHIELD, 0, 40).is_none());

    // An icon is an index into the 16 icons, or 255. A page has 4 icon slots.
    ui.open_icons(1);
    let (_, field, list) = fields_of(ui.set_icon(&screen_input, LEAKSHIELD, 1, 15));
    assert_eq!(field, "page_icons");
    assert_eq!((list.len(), list[2 * 4 + 1]), (40, 15.0));
    assert!(ui.dialog.is_none(), "the pick closes the icon dialog");
    assert!(ui.set_icon(&screen_input, LEAKSHIELD, 0, 16).is_none());
    let (_, _, list) = fields_of(ui.set_icon(&screen_input, LEAKSHIELD, 0, NO_ASSET));
    assert_eq!(list[2 * 4], 255.0);

    // "No page" is the template 255 of the registry.
    ui.gallery = true;
    let Some(Command::SetLeakshieldFields {
        fields, control, ..
    }) = ui.set_template(&screen_input, LEAKSHIELD, NO_TEMPLATE)
    else {
        panic!("a field write");
    };
    assert_eq!(control, "display:page:2");
    assert_eq!(
        fields[0].1,
        FieldValue::TextList(vec![NO_TEMPLATE.to_owned(); 10])
    );
    assert!(!ui.gallery);

    // The text list has the names of the device that are not empty, then "No text".
    let page = display(&state, &ui).page;
    let names: Vec<&str> = page.texts[0].choices.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(
        names,
        [
            "Pressure",
            "Fill level",
            "Volume change",
            "Pressure change",
            "Pump speed",
            "Flow",
            "Loop",
            "Coolant",
            "Radiator",
            "LED strip",
            "No text"
        ]
    );
    assert_eq!(page.texts[0].shown, "No text");
    assert_eq!(page.icons[0].name, "No icon");
    assert!(page.icons[0].bitmap.is_none());

    // Each write of a page names the page: the app reads its preview after the write.
    let command = ui.set_decimals(&screen_input, LEAKSHIELD, 0, 1).unwrap();
    assert_eq!(command.preview_page(), Some((LEAKSHIELD, 2)));
    assert_eq!(
        command.feedback_key().as_deref(),
        Some(format!("ls:{LEAKSHIELD}:display:page:2").as_str())
    );
    // A write of the display settings names no page.
    let flag =
        nicecooling::screens::leakshield::display::set_flag(&screen_input, LEAKSHIELD, 4, true);
    assert_eq!(flag.unwrap().preview_page(), None);
}

#[test]
fn an_own_text_goes_into_a_free_page_text() {
    // The fixture: page 2 has "1 value", page 3 has "2 values". The page texts 21 to 23 of the
    // device are "Loop", "Coolant" and "Radiator" (names 20 to 22). No page uses a text.
    let mut state = with_leakshield();
    let feedback = Feedback::default();
    let data = |state: &AppState| state.snapshot.leakshield_data[LEAKSHIELD].clone();
    // The first empty page text is name 23.
    assert_eq!(text_target(&data(&state), 1, 0), Some(23));

    let mut ui = DisplayUi::default();
    ui.select(1);
    ui.open_own_text(0);
    ui.type_own_text("CPU loop");
    let Some(DialogModel::OwnText {
        note,
        save,
        value,
        text,
        ..
    }) = display(&state, &ui).dialog
    else {
        panic!("the text dialog");
    };
    assert_eq!(value, "CPU loop");
    assert_eq!(text, "Text 1 of page 2. A text has at most 23 characters.");
    assert_eq!(
        note.as_deref(),
        Some("The text goes into page text 4 of the Leakshield.")
    );
    assert!(save.enabled);
    let command = ui
        .save_own_text(&en(&state, &feedback), LEAKSHIELD)
        .expect("a command");
    let Command::SetDisplayName {
        page,
        index,
        text,
        select,
        ..
    } = &command
    else {
        panic!("a name write");
    };
    assert_eq!((*page, *index, text.as_str()), (1, 23, "CPU loop"));
    // The slot of the page then selects the name: entry 8 of `page_names`.
    let select = select.as_ref().expect("the page selects the new name");
    assert_eq!((select.len(), select[8]), (80, 23.0));
    assert_eq!(select.iter().filter(|v| **v != 255.0).count(), 1);
    assert!(ui.dialog.is_none());
    assert_eq!(command.preview_page(), Some((LEAKSHIELD, 1)));

    // A slot that shows a page text of its own takes the new text in place: no field write.
    let mut names = vec![255.0; 80];
    names[8] = 21.0;
    set_field(&mut state, "page_names", FieldValue::List(names.clone()));
    assert_eq!(text_target(&data(&state), 1, 0), Some(21));
    ui.open_own_text(0);
    ui.type_own_text("Water");
    let command = ui
        .save_own_text(&en(&state, &feedback), LEAKSHIELD)
        .unwrap();
    assert!(matches!(
        &command,
        Command::SetDisplayName {
            index: 21,
            select: None,
            ..
        }
    ));
    // A text that another page shows too stays as it is: the new text takes a free name.
    names[16] = 21.0;
    set_field(&mut state, "page_names", FieldValue::List(names));
    assert_eq!(text_target(&data(&state), 1, 0), Some(23));
    // A sensor name (0 to 19) is never rewritten.
    let mut names = vec![255.0; 80];
    names[8] = 0.0;
    set_field(&mut state, "page_names", FieldValue::List(names));
    assert_eq!(text_target(&data(&state), 1, 0), Some(23));

    // A text of more than 23 characters, and a control character: "Save" is disabled with the
    // reason, and nothing is sent.
    ui.open_own_text(0);
    ui.type_own_text("A text of 24 characters.");
    let Some(DialogModel::OwnText { save, .. }) = display(&state, &ui).dialog else {
        panic!("the text dialog");
    };
    assert!(!save.enabled);
    assert_eq!(
        save.reason.as_deref(),
        Some("The text has more than 23 characters.")
    );
    assert!(
        ui.save_own_text(&en(&state, &feedback), LEAKSHIELD)
            .is_none()
    );
    ui.type_own_text("tab\there");
    let Some(DialogModel::OwnText { save, .. }) = display(&state, &ui).dialog else {
        panic!("the text dialog");
    };
    assert_eq!(
        save.reason.as_deref(),
        Some("The text has a control character.")
    );
    ui.type_own_text("A text of 23 characters");
    assert!(
        ui.save_own_text(&en(&state, &feedback), LEAKSHIELD)
            .is_some()
    );

    // All 15 page texts in use: four pages with 4 texts each show the names 20 to 34.
    let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
    for template in templates.iter_mut().take(4) {
        *template = "values-4".into();
    }
    templates[4] = "values-1".into();
    set_field(&mut state, "page_template", FieldValue::TextList(templates));
    let mut names = vec![255.0; 80];
    for (index, name) in (20..35).enumerate() {
        names[index / 4 * 8 + index % 4] = f64::from(name);
    }
    set_field(&mut state, "page_names", FieldValue::List(names));
    assert_eq!(text_target(&data(&state), 4, 0), None);
    ui.select(4);
    let page = display(&state, &ui).page;
    assert!(!page.texts[0].own.enabled);
    assert_eq!(
        page.texts[0].own.reason.as_deref(),
        Some("All 15 page texts are in use. Pick one of the texts of the list.")
    );
    // A slot that has one of the 15 texts for itself still takes an own text.
    assert_eq!(text_target(&data(&state), 0, 0), Some(20));
}

#[test]
fn an_own_text_does_not_take_the_name_of_a_page_that_is_off() {
    // Page 5 has the template "none", but its `page_names` still refer to the page text 23.
    let mut state = with_leakshield();
    let data = |state: &AppState| state.snapshot.leakshield_data[LEAKSHIELD].clone();
    assert_eq!(text_target(&data(&state), 1, 0), Some(23));
    let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
    templates[1] = "values-1".into();
    set_field(&mut state, "page_template", FieldValue::TextList(templates));
    let mut names = vec![255.0; 80];
    names[4 * 8] = 23.0;
    set_field(&mut state, "page_names", FieldValue::List(names));

    assert_eq!(text_target(&data(&state), 1, 0), Some(24));
}

// ---------------------------------------------------------------------------------------------
// The states of a preview.

#[test]
fn each_preview_without_an_image_has_its_reason() {
    let mut state = with_leakshield();
    // Not read yet: the tile and the selected page say so.
    let model = display(&state, &DisplayUi::default());
    assert_eq!(model.pages[0].screen, Screen::Note("Not read yet".into()));
    assert_eq!(
        model.page.preview.note.as_deref(),
        Some("No image yet. nicecooling reads it when you select the page.")
    );
    assert!(model.page.preview.refresh.enabled);
    assert_eq!(model.page.preview.refresh.label, "Refresh preview");
    assert_eq!(
        model.page.preview.caption,
        "The Leakshield shows the page for 30 s each time nicecooling reads its image."
    );
    // A page without a template: "Empty", and no read.
    assert_eq!(model.pages[3].screen, Screen::Note("Empty".into()));
    let mut ui = DisplayUi::default();
    ui.select(3);
    let empty = display(&state, &ui);
    assert!(!empty.page.preview.refresh.enabled);
    assert_eq!(
        empty.page.preview.refresh.reason.as_deref(),
        Some("This page has no template. The Leakshield skips it.")
    );
    assert!(
        ui.preview_command(&en(&state, &Feedback::default()), LEAKSHIELD)
            .is_none()
    );

    // The image of the device.
    let mut image = Bitmap::new(128, 64);
    image.set(5, 5, true);
    preview(&mut state, 0, PagePreview::Image(image.clone()));
    let model = display(&state, &DisplayUi::default());
    assert_eq!(model.pages[0].screen, Screen::Image(image.clone()));
    assert_eq!(model.page.preview.screen, Screen::Image(image.clone()));
    assert!(model.page.preview.note.is_none() && !model.page.preview.loading);
    // The image of page 1 is not the image of page 2.
    assert_eq!(model.pages[1].screen, Screen::Note("Not read yet".into()));

    // While a read runs, the old image stays and the button waits.
    let mut reading = Feedback::default();
    reading.set(
        format!("ls:{LEAKSHIELD}:display:preview:0"),
        WriteOutcome::Pending,
    );
    let model = DisplayModel::build(&en(&state, &reading), &DisplayUi::default(), LEAKSHIELD);
    assert_eq!(model.page.preview.screen, Screen::Image(image));
    assert!(model.page.preview.loading && !model.page.preview.refresh.enabled);
    assert_eq!(
        model.page.preview.note.as_deref(),
        Some("nicecooling reads the page from the Leakshield.")
    );
    // A write of the page reads its image again: the same state.
    let mut writing = Feedback::default();
    writing.set(
        format!("ls:{LEAKSHIELD}:display:page:1"),
        WriteOutcome::Pending,
    );
    let model = DisplayModel::build(&en(&state, &writing), &DisplayUi::default(), LEAKSHIELD);
    assert!(model.pages[1].loading && !model.pages[0].loading);
    assert_eq!(model.pages[1].screen, Screen::Note("Reading".into()));

    // Each error of the daemon has a short text for the tile and a reason for the page.
    for (error, short, reason) in [
        (
            "PageNotShown",
            "Not shown",
            "The Leakshield does not show this page now: its display has another page, or none.",
        ),
        (
            "ReadOnly",
            "Read-only",
            "The Leakshield is read-only. nicecooling sends it no command, so it cannot ask for the page.",
        ),
        (
            "Disconnected",
            "No answer",
            "The Leakshield does not answer.",
        ),
        (
            "UnknownMethod",
            "Not available",
            "This daemon cannot read the display. Update nicecoolingd.",
        ),
        (
            "NotSupported",
            "Not supported",
            "This firmware has no command that shows a page.",
        ),
        ("Busy", "Read failed", "The read failed: Busy"),
    ] {
        preview(&mut state, 0, PagePreview::Failed(error.into()));
        let model = display(&state, &DisplayUi::default());
        assert_eq!(model.pages[0].screen, Screen::Note(short.into()), "{error}");
        assert_eq!(model.page.preview.note.as_deref(), Some(reason), "{error}");
        // No preview is a black rectangle without a reason.
        assert!(matches!(model.page.preview.screen, Screen::Note(_)));
    }

    // Italian has the same states.
    let model = DisplayModel::build(
        &input(&state, &Feedback::default(), IT),
        &DisplayUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        model.pages[0].screen,
        Screen::Note("Lettura fallita".into())
    );
    assert_eq!(
        model.pages[1].screen,
        Screen::Note("Non ancora letta".into())
    );
    assert_eq!(model.page.preview.refresh.label, "Aggiorna anteprima");
}

#[test]
fn a_write_that_is_not_possible_disables_its_control_with_the_reason() {
    let feedback = Feedback::default();
    let cases = [
        (
            unknown_firmware(),
            "Read-only: unknown firmware.",
            "Read-only",
        ),
        (
            device_away(),
            "The Leakshield does not answer. Check its USB cable.",
            "No answer",
        ),
        (daemon_offline(), "The daemon is offline.", "Daemon offline"),
    ];
    for (mut state, reason, short) in cases {
        // Page 3 of the fixture has "2 values": texts and icons. Page 1 gets the logo.
        let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
        let field = data
            .fields
            .iter_mut()
            .find(|f| f.id == "page_template")
            .unwrap();
        if let FieldValue::TextList(list) = &mut field.value {
            list[0] = "logo".into();
        }
        let screen_input = en(&state, &feedback);
        let mut ui = DisplayUi::default();
        ui.select(2);
        let model = DisplayModel::build(&screen_input, &ui, LEAKSHIELD);
        let page = &model.page;
        let some = Some(reason.to_owned());
        // The preview read is a command: the device takes none. A page without an image says
        // that state, not "Not read yet".
        assert_eq!(
            model.pages[1].screen,
            Screen::Note(short.into()),
            "{reason}"
        );
        assert_eq!(page.preview.screen, Screen::Note(short.into()), "{reason}");
        assert!(page.preview.note.is_some(), "{reason}");
        assert!(!page.preview.refresh.enabled, "{reason}");
        assert_eq!(page.preview.refresh.reason, some);
        assert!(ui.preview_command(&screen_input, LEAKSHIELD).is_none());
        // The template, the options and every slot.
        assert!(
            !page.change.enabled && page.change.reason == some,
            "{reason}"
        );
        assert!(!page.hide_normal.writable && !page.standby.writable);
        assert!(
            !page.sources_writable
                && !page.decimals_writable
                && !page.ranges_writable
                && !page.texts_writable
        );
        assert!(
            page.texts
                .iter()
                .all(|t| !t.own.enabled && t.own.reason == some)
        );
        assert!(
            page.icons
                .iter()
                .all(|i| !i.change.enabled && i.change.reason == some)
        );
        ui.select(0);
        let logo = DisplayModel::build(&screen_input, &ui, LEAKSHIELD)
            .page
            .logo
            .expect("the logo row");
        assert!(
            !logo.change.enabled && logo.change.reason == some,
            "{reason}"
        );
        // No command leaves the app.
        ui.select(2);
        assert!(ui.set_template(&screen_input, LEAKSHIELD, "time").is_none());
        assert!(ui.set_decimals(&screen_input, LEAKSHIELD, 0, 1).is_none());
        assert!(
            ui.set_range(&screen_input, LEAKSHIELD, 0, true, 5.0)
                .is_none()
        );
        assert!(ui.set_text(&screen_input, LEAKSHIELD, 0, 1).is_none());
        assert!(ui.set_icon(&screen_input, LEAKSHIELD, 0, 1).is_none());
        assert!(ui.set_option(&screen_input, LEAKSHIELD, 1, false).is_none());
        ui.open_own_text(0);
        ui.type_own_text("Text");
        let Some(DialogModel::OwnText { save, .. }) =
            DisplayModel::build(&screen_input, &ui, LEAKSHIELD).dialog
        else {
            panic!("the text dialog");
        };
        assert!(!save.enabled && save.reason == some, "{reason}");
        assert!(ui.save_own_text(&screen_input, LEAKSHIELD).is_none());
        ui.open_import(ImportTarget::Logo);
        ui.import_loaded(
            "logo.png".into(),
            import_png(&png(128, 64, |_, _| WHITE), 128, 64),
        );
        let Some(DialogModel::Import { write, .. }) =
            DisplayModel::build(&screen_input, &ui, LEAKSHIELD).dialog
        else {
            panic!("the import dialog");
        };
        assert!(!write.enabled && write.reason == some, "{reason}");
        assert!(ui.write_import(&screen_input, LEAKSHIELD).is_none());

        // The reset of the event list is a command too.
        let status = StatusModel::build(&screen_input, &StatusUi::default(), LEAKSHIELD);
        assert!(!status.events.reset_enabled, "{reason}");
        assert_eq!(status.events.reset_reason, some);
    }

    // A device that takes writes, with one field that is not a Verified field: only the
    // controls of that field are disabled, with the reason of the field.
    let mut state = with_leakshield();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    let field = data
        .fields
        .iter_mut()
        .find(|f| f.id == "page_names")
        .unwrap();
    field.writable = false;
    field.reason = "not-verified".into();
    let mut ui = DisplayUi::default();
    ui.select(2);
    let page = display(&state, &ui).page;
    assert!(!page.texts_writable && page.sources_writable && page.change.enabled);
    assert_eq!(
        page.texts[0].own.reason.as_deref(),
        Some("Not a Verified field: this Safety field has no Verification write yet.")
    );
    assert!(page.icons[0].change.enabled && page.preview.refresh.enabled);

    // A daemon that gave no names and no icons: an own text has no target.
    let mut state = with_leakshield();
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .display_assets = None;
    let page = display(&state, &ui).page;
    assert!(!page.texts[0].own.enabled);
    assert_eq!(
        page.texts[0].own.reason.as_deref(),
        Some("nicecooling has no texts and no icons of this Leakshield.")
    );
    // The list then has the 40 names by number.
    assert_eq!(page.texts[0].choices.len(), 41);
    assert_eq!(page.texts[0].choices[20].1, "Name 21");
}

// ---------------------------------------------------------------------------------------------
// The other tabs.

#[test]
fn the_sensors_tab_has_scale_offset_and_the_sources_of_the_daemon() {
    let mut state = with_leakshield();
    let feedback = Feedback::default();
    let sensors = |state: &AppState| {
        SensorsModel::build(&en(state, &feedback), &SensorsUi::default(), LEAKSHIELD)
    };
    let model = sensors(&state);
    assert_eq!(
        model.software_columns,
        [
            "Slot",
            "Name",
            "Data source",
            "Unit",
            "Scale",
            "Offset",
            "Value"
        ]
    );
    // Slot 3 of the fixture sends the water temperature: 31.4 °C at scale 1 and offset 0.
    let row = &model.software_rows[2];
    assert_eq!((row.scale.as_str(), row.offset.as_str()), ("1", "0"));
    assert_eq!(row.value, "31.4 °C");
    let all = row.choices.len();

    // The value that the daemon sends is the source value times the scale plus the offset.
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.software_scales = vec![(3, 2.0, -10.5)];
    let model = sensors(&state);
    let row = &model.software_rows[2];
    assert_eq!((row.scale.as_str(), row.offset.as_str()), ("2", "-10.5"));
    assert_eq!(row.value, "52.3 °C");
    // A slot that the daemon does not list has scale 1 and offset 0.
    assert_eq!(model.software_rows[0].scale, "1");

    // A new scale keeps the offset, and a new offset keeps the scale.
    assert_eq!(
        set_scale(&state, LEAKSHIELD, 3, false, 0.5),
        Some(Command::SetSoftwareSensorScale {
            leakshield: LEAKSHIELD.into(),
            slot: 3,
            scale: 0.5,
            offset: -10.5,
        })
    );
    assert_eq!(
        set_scale(&state, LEAKSHIELD, 3, true, 4.0),
        Some(Command::SetSoftwareSensorScale {
            leakshield: LEAKSHIELD.into(),
            slot: 3,
            scale: 2.0,
            offset: 4.0,
        })
    );
    // The same value and a number that is not finite send nothing.
    assert_eq!(set_scale(&state, LEAKSHIELD, 3, false, 2.0), None);
    assert_eq!(set_scale(&state, LEAKSHIELD, 3, true, f64::INFINITY), None);
    let command = set_scale(&state, LEAKSHIELD, 5, false, 3.0).unwrap();
    assert_eq!(
        command.feedback_key().as_deref(),
        Some(format!("ls:{LEAKSHIELD}:software-sensor:5").as_str())
    );

    // The picker lists the sensors that the daemon takes. The source of a slot stays in the
    // list, also when the daemon no longer takes it.
    let source = row.source.clone().expect("the source of slot 3");
    let other = state
        .snapshot
        .sensors
        .keys()
        .find(|id| **id != source)
        .cloned()
        .unwrap();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.software_sources = Some(vec![other.clone()]);
    let model = sensors(&state);
    assert!(all > 2);
    let ids = |slot: usize| -> Vec<String> {
        model.software_rows[slot]
            .choices
            .iter()
            .map(|c| c.0.clone())
            .collect()
    };
    assert_eq!(ids(0), std::slice::from_ref(&other));
    let mut with_own = ids(2);
    with_own.sort();
    let mut expected = vec![other, source];
    expected.sort();
    assert_eq!(with_own, expected);
}

#[test]
fn the_system_tab_has_the_device_state_card() {
    let mut state = with_leakshield();
    let feedback = Feedback::default();
    let system = |state: &AppState, locale: Locale| {
        SystemModel::build(
            &input(state, &feedback, locale),
            &SystemUi::default(),
            LEAKSHIELD,
        )
    };
    let card = system(&state, EN).state_card;
    assert_eq!(card.title, "Device state");
    let names: Vec<&str> = card.rows.iter().map(|row| row.0.as_str()).collect();
    assert_eq!(
        names,
        [
            "Maintenance indicator",
            "Vacuum pump runtime",
            "Vacuum pump runtime in short cycles",
            "Vacuum pump revolutions",
            "Fill valve cycles",
            "Vent valve cycles",
            "Operating time",
            "Power-ups"
        ]
    );
    // The counters of the fixture: 11431 s is 3 h 10 min, 21 s is 0 min 21 s.
    let values: Vec<&str> = card.rows.iter().skip(1).map(|row| row.1.as_str()).collect();
    assert_eq!(
        values,
        [
            "3 h 10 min",
            "0 min 21 s",
            "1\u{a0}234\u{a0}567",
            "12",
            "345",
            "274 h 20 min",
            "42"
        ]
    );
    assert!(card.note.is_none() && !card.dim);
    // The maintenance value is the one of the Device card.
    assert_eq!(card.rows[0].1, system(&state, EN).device.maintenance_text);
    assert_eq!(duration_text(EN, 3_600_000), "1\u{a0}000 h 0 min");
    assert_eq!(duration_text(EN, 59), "0 min 59 s");
    assert_eq!(system(&state, IT).state_card.title, "Stato del dispositivo");

    // A daemon that gives no counters: "--" and the reason.
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .counters = None;
    let card = system(&state, EN).state_card;
    assert!(card.rows.iter().skip(1).all(|row| row.1 == "--"));
    assert_eq!(card.note.as_deref(), Some("The daemon gave no counters."));
    // A Leakshield that does not answer: the values are dim and the note is the banner.
    let card = system(&device_away(), EN).state_card;
    assert!(card.dim && card.note.is_none());
}

#[test]
fn the_status_tab_resets_the_event_list_after_a_question() {
    let state = with_leakshield();
    let feedback = Feedback::default();
    let mut ui = StatusUi::default();
    let status = |ui: &StatusUi| StatusModel::build(&en(&state, &feedback), ui, LEAKSHIELD).events;
    let events = status(&ui);
    assert_eq!(events.reset, "Reset");
    assert!(events.reset_enabled && events.reset_reason.is_none() && events.dialog.is_none());
    // The click asks first.
    ui.request_reset();
    let dialog = status(&ui).dialog.expect("the question");
    assert_eq!(dialog.title, "Delete the event list?");
    assert_eq!(
        dialog.text,
        "The Leakshield deletes every event of its list. This cannot be undone. Export the list first when you need it."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Delete events")
    );
    // "Cancel" sends nothing.
    ui.cancel_reset();
    assert!(status(&ui).dialog.is_none());
    assert_eq!(ui.confirm_reset(LEAKSHIELD), None);
    // The answer sends the command once.
    ui.request_reset();
    let command = ui.confirm_reset(LEAKSHIELD).expect("a command");
    assert_eq!(
        command,
        Command::ResetEventLog {
            leakshield: LEAKSHIELD.into()
        }
    );
    assert!(!ui.ask_reset && ui.confirm_reset(LEAKSHIELD).is_none());
    assert_eq!(
        command.leakshield(),
        Some(LEAKSHIELD),
        "the app reads the list again"
    );
    // The result shows in the card.
    let mut refused = Feedback::default();
    refused.set(
        command.feedback_key().unwrap(),
        WriteOutcome::Refused {
            error: "NotSupported".into(),
        },
    );
    let events = StatusModel::build(&en(&state, &refused), &ui, LEAKSHIELD).events;
    let line = events.reset_line.expect("the result");
    assert!(line.failure);
    assert_eq!(line.text, "This firmware has no such command.");
}

#[test]
fn the_overview_has_pause_or_resume_while_a_refill_runs() {
    let mut state = with_leakshield();
    let feedback = Feedback::default();
    let overview = |state: &AppState| {
        OverviewModel::build(&en(state, &feedback), &OverviewUi::default(), LEAKSHIELD).mode
    };
    // No Refill: no button.
    assert!(overview(&state).refill_pause.is_none());
    state.snapshot.leakshields[0].running_action = "test".into();
    assert!(overview(&state).refill_pause.is_none());
    // The Refill runs: one button for both, because the device toggles and the daemon does not
    // say which state the Refill is in.
    state.snapshot.leakshields[0].running_action = "refill".into();
    let mode = overview(&state);
    let pause = mode.refill_pause.expect("the button");
    assert_eq!(pause.label, "Pause or resume");
    assert_eq!(
        pause.text,
        "The Leakshield pauses a running Refill and resumes a paused one."
    );
    assert!(pause.enabled && pause.reason.is_none());
    assert!(mode.cancel.is_some(), "Cancel stays next to it");
    let command = Command::ToggleRefillPause {
        leakshield: LEAKSHIELD.into(),
    };
    assert_eq!(
        command.feedback_key().as_deref(),
        Some(format!("ls:{LEAKSHIELD}:refill-pause").as_str())
    );
    // A read-only device takes no command: the button shows with the reason.
    let mut unknown = unknown_firmware();
    unknown.snapshot.leakshields[0].running_action = "refill".into();
    let pause = overview(&unknown).refill_pause.expect("the button");
    assert!(!pause.enabled);
    assert_eq!(
        pause.reason.as_deref(),
        Some("Read-only: unknown firmware.")
    );
    // A Leakshield that does not answer has no action bar for the Refill.
    let mut away = device_away();
    away.snapshot.leakshields[0].running_action = "refill".into();
    assert!(overview(&away).refill_pause.is_none());
}

// ---------------------------------------------------------------------------------------------
// The calls of the daemon.

/// A Harness on a private bus with a fake daemon, once the Leakshield data is read.
async fn started(options: Options) -> (PrivateBus, FakeDaemon, Harness) {
    let bus = PrivateBus::start("leakshield-display");
    let daemon = FakeDaemon::start(bus.connect().await, options).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Leakshield data is read", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|data| !data.fields.is_empty())
        })
        .await;
    (bus, daemon, harness)
}

/// Run a command as the app does: the write, the read of the device data and the refresh.
async fn run(harness: &mut Harness, command: &Command) -> WriteOutcome {
    let outcome = match command.run_on_daemon(&harness.client).await {
        Ok(_) => WriteOutcome::Applied { at_us: NOW_US },
        Err(error) => WriteOutcome::from_error(&error),
    };
    if let Some(id) = command.leakshield() {
        let data = harness.client.leakshield_data(id).await;
        harness
            .state
            .snapshot
            .leakshield_data
            .insert(id.to_owned(), data);
    }
    if let Some(event) = command.refresh(&harness.client).await {
        harness.state.apply(&event);
    }
    outcome
}

fn has_call(daemon: &FakeDaemon, call: &str) -> bool {
    daemon.calls().iter().any(|made| made.starts_with(call))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_snapshot_reads_the_assets_the_counters_the_scales_and_the_sources() {
    let (_bus, _daemon, harness) = started(Options::default()).await;
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    let assets = data.display_assets.as_ref().expect("the assets");
    assert_eq!(assets.names.len(), 40);
    assert_eq!(assets.names[0], "Pressure");
    assert_eq!(assets.names[21], "Coolant");
    assert_eq!(assets.icons.len(), 16);
    assert!(assets.icons.iter().all(|icon| {
        (icon.width, icon.height, icon.rows.len()) == (23, 23, 69) && icon.is_valid()
    }));
    assert_eq!(
        (
            assets.logo.width,
            assets.logo.height,
            assets.logo.rows.len()
        ),
        (128, 64, 1024)
    );
    let counters = data.counters.as_ref().expect("the counters");
    assert_eq!(counters["pump_runtime_s"], 11431);
    assert_eq!(counters.len(), 7);
    assert_eq!(data.software_scales.len(), 12);
    assert_eq!(data.software_scales[0], (1, 1.0, 0.0));
    let sources = data.software_sources.as_ref().expect("the sources");
    assert!(sources.contains(&"nct6798_temp1".to_owned()));
    assert!(!sources.contains(&"loop_1_flow".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_read_stores_the_image_or_the_error() {
    let (_bus, daemon, mut harness) = started(Options::default()).await;
    let feedback = Feedback::default();
    let mut ui = DisplayUi::default();
    ui.select(1);
    let command = ui
        .preview_command(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    assert_eq!(
        command,
        Command::ReadDisplayPreview {
            leakshield: LEAKSHIELD.into(),
            page: 1,
        }
    );
    assert_eq!(
        command.feedback_key().as_deref(),
        Some(format!("ls:{LEAKSHIELD}:display:preview:1").as_str())
    );
    assert_eq!(
        command.leakshield(),
        None,
        "a preview read reads no settings again"
    );
    run(&mut harness, &command).await;
    assert!(has_call(&daemon, "GetDisplayPreview(1)"));
    let Some(PagePreview::Image(image)) = harness
        .state
        .display_previews
        .get(&(LEAKSHIELD.to_owned(), 1))
    else {
        panic!("the image of page 2");
    };
    // The image of the fake device: a frame, and 2 bars for page index 1 from column 8.
    assert_eq!((image.width, image.height), (128, 64));
    assert!(image.pixel(0, 0) && image.pixel(127, 63) && image.pixel(8, 8));
    assert!(image.pixel(14, 8) && !image.pixel(20, 8), "two bars");
    assert!(!image.pixel(8, 40), "the bars are in the top half");
    let model = display(&harness.state, &ui);
    assert_eq!(model.page.preview.screen, Screen::Image(image.clone()));
    assert_eq!(model.pages[1].screen, Screen::Image(image.clone()));

    // A page that the device does not show: the error is the state of the preview.
    daemon.hide_pages(&[1]).await;
    run(&mut harness, &command).await;
    assert_eq!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 1)),
        Some(&PagePreview::Failed("PageNotShown".into()))
    );
    assert_eq!(
        display(&harness.state, &ui).pages[1].screen,
        Screen::Note("Not shown".into())
    );
    daemon.hide_pages(&[]).await;

    // A page without a template gives the same error when it is asked for.
    let empty = Command::ReadDisplayPreview {
        leakshield: LEAKSHIELD.into(),
        page: 8,
    };
    run(&mut harness, &empty).await;
    assert_eq!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 8)),
        Some(&PagePreview::Failed("PageNotShown".into()))
    );

    // A device that went away.
    daemon.set_connected(false).await;
    let event = command.refresh(&harness.client).await.expect("an event");
    assert_eq!(
        event,
        DaemonEvent::DisplayPreview {
            leakshield: LEAKSHIELD.into(),
            page: 1,
            preview: PagePreview::Failed("Disconnected".into()),
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_device_and_a_daemon_without_the_methods_have_their_state() {
    // A read-only device: the app sends no preview read. A read that is sent anyway fails
    // with `ReadOnly`.
    let (_bus, daemon, mut harness) = started(Options {
        unknown_firmware: true,
        ..Options::default()
    })
    .await;
    let feedback = Feedback::default();
    let ui = DisplayUi::default();
    assert!(
        ui.preview_command(&en(&harness.state, &feedback), LEAKSHIELD)
            .is_none()
    );
    let model = display(&harness.state, &ui);
    assert!(!model.page.preview.refresh.enabled);
    assert_eq!(
        model.page.preview.refresh.reason.as_deref(),
        Some("Read-only: unknown firmware.")
    );
    let forced = Command::ReadDisplayPreview {
        leakshield: LEAKSHIELD.into(),
        page: 0,
    };
    run(&mut harness, &forced).await;
    assert_eq!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 0)),
        Some(&PagePreview::Failed("ReadOnly".into()))
    );
    assert_eq!(
        display(&harness.state, &ui).pages[0].screen,
        Screen::Note("Read-only".into())
    );
    // The writes of report 8 and the commands are refused too.
    let name = Command::SetDisplayName {
        leakshield: LEAKSHIELD.into(),
        page: 0,
        index: 20,
        text: "Text".into(),
        select: None,
    };
    assert_eq!(
        run(&mut harness, &name).await,
        WriteOutcome::Refused {
            error: "ReadOnly".into()
        }
    );
    assert_eq!(daemon.display_names().await[20], "Loop", "nothing changed");
    drop(harness);

    // A daemon of before the display methods.
    let (_bus, daemon, mut harness) = started(Options {
        no_display_methods: true,
        ..Options::default()
    })
    .await;
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert!(data.display_assets.is_none() && data.counters.is_none());
    assert!(data.software_sources.is_none() && data.software_scales.is_empty());
    assert!(!data.fields.is_empty(), "the other parts are read");
    let read = ui
        .preview_command(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("the device takes commands");
    run(&mut harness, &read).await;
    assert_eq!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 0)),
        Some(&PagePreview::Failed("UnknownMethod".into()))
    );
    let model = display(&harness.state, &ui);
    assert_eq!(model.pages[0].screen, Screen::Note("Not available".into()));
    assert_eq!(
        model.page.preview.note.as_deref(),
        Some("This daemon cannot read the display. Update nicecoolingd.")
    );
    assert!(!has_call(&daemon, "GetDisplayPreview"));
    // Without a source list every sensor is a source, as before.
    let sensors = SensorsModel::build(
        &en(&harness.state, &feedback),
        &SensorsUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        sensors.software_rows[0].choices.len(),
        harness.state.snapshot.sensors.len()
    );
    let system = SystemModel::build(
        &en(&harness.state, &feedback),
        &SystemUi::default(),
        LEAKSHIELD,
    );
    assert_eq!(
        system.state_card.note.as_deref(),
        Some("The daemon gave no counters.")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_new_commands_reach_the_fake_daemon_with_their_arguments() {
    let (_bus, daemon, mut harness) = started(Options::default()).await;
    let feedback = Feedback::default();
    let applied = WriteOutcome::Applied { at_us: NOW_US };

    // An own text: the name of report 8, then the field that selects it. Page 2 of the fake
    // device has "1 value".
    let mut ui = DisplayUi::default();
    ui.select(1);
    ui.open_own_text(0);
    ui.type_own_text("CPU loop");
    let command = ui
        .save_own_text(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(
        &daemon,
        "SetDisplayName(23,CPU loop) interactive=true"
    ));
    assert_eq!(daemon.display_names().await[23], "CPU loop");
    let Some(support::fake_leakshield::Fv::Ints(names)) = daemon.stored_field("page_names").await
    else {
        panic!("the names of the pages");
    };
    assert_eq!(names[8], 23);
    // The app reads the assets and the preview of the page again.
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert_eq!(data.display_assets.as_ref().unwrap().names[23], "CPU loop");
    assert!(has_call(&daemon, "GetDisplayPreview(1)"));
    assert!(matches!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 1)),
        Some(PagePreview::Image(_))
    ));
    let page = display(&harness.state, &ui).page;
    assert_eq!(page.texts[0].shown, "CPU loop");

    // The slots of a page, each as one `SetFields` call. The preview follows each write.
    let before = daemon.calls().len();
    let command = ui
        .set_decimals(&en(&harness.state, &feedback), LEAKSHIELD, 0, 2)
        .unwrap();
    assert_eq!(run(&mut harness, &command).await, applied);
    let calls = daemon.calls();
    assert!(
        calls[before].starts_with("SetFields(page_precisions=["),
        "{calls:?}"
    );
    assert_eq!(calls[before + 1], "GetDisplayPreview(1)");
    let Some(support::fake_leakshield::Fv::Ints(decimals)) =
        daemon.stored_field("page_precisions").await
    else {
        panic!("the decimals");
    };
    assert_eq!(decimals[8], 2);
    let command = ui
        .set_range(&en(&harness.state, &feedback), LEAKSHIELD, 0, true, 450.0)
        .unwrap();
    assert_eq!(run(&mut harness, &command).await, applied);
    let Some(support::fake_leakshield::Fv::Ints(limits)) = daemon.stored_field("page_values").await
    else {
        panic!("the limits");
    };
    assert_eq!(limits[8..10], [0, 450]);
    let command = ui
        .set_icon(&en(&harness.state, &feedback), LEAKSHIELD, 0, 9)
        .unwrap();
    assert_eq!(run(&mut harness, &command).await, applied);
    let Some(support::fake_leakshield::Fv::Ints(icons)) = daemon.stored_field("page_icons").await
    else {
        panic!("the icons");
    };
    assert_eq!(icons[4], 9);

    // An icon and the logo: the size and the byte count of the contract.
    ui.open_import(ImportTarget::Icon { slot: 0 });
    ui.import_loaded(
        "icon.png".into(),
        import_png(&png(23, 23, |_, _| WHITE), 23, 23),
    );
    let command = ui
        .write_import(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(
        &daemon,
        "SetDisplayIcon(9,23,23,69) interactive=true"
    ));
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert_eq!(
        data.display_assets.as_ref().unwrap().icons[9].lit_count(),
        23 * 23
    );

    ui.open_import(ImportTarget::Logo);
    ui.import_loaded(
        "logo.png".into(),
        import_png(&png(640, 320, |_, _| WHITE), 128, 64),
    );
    let command = ui
        .write_import(&en(&harness.state, &feedback), LEAKSHIELD)
        .expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(
        &daemon,
        "SetDisplayLogo(128,64,1024) interactive=true"
    ));
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert_eq!(
        data.display_assets.as_ref().unwrap().logo.lit_count(),
        128 * 64
    );

    // "No page": the template 255.
    ui.select(4);
    let command = ui
        .set_template(&en(&harness.state, &feedback), LEAKSHIELD, NO_TEMPLATE)
        .unwrap();
    assert_eq!(run(&mut harness, &command).await, applied);
    let Some(support::fake_leakshield::Fv::TextList(templates)) =
        daemon.stored_field("page_template").await
    else {
        panic!("the templates");
    };
    assert_eq!(templates[4], "none");
    // The device does not show an empty page: the preview says so.
    assert_eq!(
        harness
            .state
            .display_previews
            .get(&(LEAKSHIELD.to_owned(), 4)),
        Some(&PagePreview::Failed("PageNotShown".into()))
    );

    // The scale and the offset of a Software sensor slot.
    let command = set_scale(&harness.state, LEAKSHIELD, 3, false, 2.5).expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(
        &daemon,
        "SetSoftwareSensorScale(3,2.5,0) interactive=true"
    ));
    let command = set_scale(&harness.state, LEAKSHIELD, 3, true, -4.0).expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(
        &daemon,
        "SetSoftwareSensorScale(3,2.5,-4) interactive=true"
    ));
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert_eq!(data.software_scales[2], (3, 2.5, -4.0));
    let outside = Command::SetSoftwareSensorScale {
        leakshield: LEAKSHIELD.into(),
        slot: 13,
        scale: 1.0,
        offset: 0.0,
    };
    assert_eq!(
        run(&mut harness, &outside).await,
        WriteOutcome::Refused {
            error: "InvalidArgument".into()
        }
    );

    // The event list reset: the list is empty after it.
    assert!(
        !harness.state.snapshot.leakshield_data[LEAKSHIELD]
            .events
            .is_empty()
    );
    let mut status = StatusUi::default();
    status.request_reset();
    let command = status.confirm_reset(LEAKSHIELD).expect("a command");
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(&daemon, "ResetEventLog() interactive=true"));
    assert!(
        harness.state.snapshot.leakshield_data[LEAKSHIELD]
            .events
            .is_empty()
    );
    let model = StatusModel::build(&en(&harness.state, &feedback), &status, LEAKSHIELD);
    assert!(model.events.rows.is_empty() && model.events.empty.is_some());

    // The Refill pause.
    let command = Command::ToggleRefillPause {
        leakshield: LEAKSHIELD.into(),
    };
    assert_eq!(run(&mut harness, &command).await, applied);
    assert!(has_call(&daemon, "ToggleRefillPause() interactive=true"));

    // polkit denies a write: the name stays.
    daemon.deny_writes(true);
    let denied = Command::SetDisplayName {
        leakshield: LEAKSHIELD.into(),
        page: 1,
        index: 23,
        text: "Other".into(),
        select: None,
    };
    assert_eq!(
        run(&mut harness, &denied).await,
        WriteOutcome::NotAuthorized
    );
    assert_eq!(daemon.display_names().await[23], "CPU loop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_firmware_without_the_commands_refuses_the_reset_and_the_pause() {
    let (_bus, _daemon, mut harness) = started(Options {
        no_reset_commands: true,
        ..Options::default()
    })
    .await;
    for command in [
        Command::ResetEventLog {
            leakshield: LEAKSHIELD.into(),
        },
        Command::ToggleRefillPause {
            leakshield: LEAKSHIELD.into(),
        },
    ] {
        assert_eq!(
            run(&mut harness, &command).await,
            WriteOutcome::Refused {
                error: "NotSupported".into()
            }
        );
    }
    assert!(
        !harness.state.snapshot.leakshield_data[LEAKSHIELD]
            .events
            .is_empty()
    );
}

/// A picture that the conversion keeps: the result of `Gray` is the input of the write.
#[test]
fn the_result_of_a_conversion_is_a_bitmap_of_the_contract() {
    let picture = Gray {
        width: 23,
        height: 23,
        luma: vec![200; 23 * 23],
        source: (23, 23),
    };
    let bitmap = picture.to_bitmap(50, false);
    assert!(bitmap.is_valid());
    assert_eq!(bitmap.rows.len(), 69);
    // The padding bit of each row stays 0.
    assert!(bitmap.rows.chunks(3).all(|row| row == [0xFF, 0xFF, 0xFE]));
}

// ---------------------------------------------------------------------------------------------
// The layout at the two window sizes, in English and in Italian.

mod layout {
    use gpui::{AppContext as _, Bounds, Pixels, TestAppContext, VisualTestContext, px};
    use nicecooling::app::{self, AppModel, Backend, StartMode};
    use nicecooling::bitmap::{Bitmap, import_png};
    use nicecooling::i18n::Language;
    use nicecooling::model::{DaemonEvent, FieldValue, PagePreview};
    use nicecooling::screen_view::ScreenView;
    use nicecooling::screens::leakshield::Tab;
    use nicecooling::screens::leakshield::display::{ImportTarget, LAYOUTS, NO_TEMPLATE};
    use tokio::sync::{broadcast, mpsc};

    use super::{LEAKSHIELD, WHITE, png, with_leakshield};

    /// The sidebar of the shell: the page is this much narrower than the window.
    const SIDEBAR: f32 = 220.0;

    struct Rig {
        window: gpui::WindowHandle<ScreenView>,
        model: gpui::Entity<AppModel>,
        events: broadcast::Sender<DaemonEvent>,
        _keep: Box<dyn std::any::Any>,
    }

    /// A tab of the Leakshield of the fixture, in a window of `size` with the sidebar.
    fn open(cx: &mut TestAppContext, language: Language, size: (f32, f32), tab: Tab) -> Rig {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (events, events_rx) = broadcast::channel(64);
        let (commands, inbox) = mpsc::unbounded_channel();
        let backend = Backend {
            events: events_rx,
            commands: commands.clone(),
            inbox,
            client: None,
            runtime: runtime.handle().clone(),
            language,
            settings: Default::default(),
        };
        let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
        let mut state = with_leakshield();
        // A Software sensor with a scale, and a running Refill for the pause button.
        let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
        data.software_scales = vec![(3, 1000.5, -1000.5)];
        events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        events
            .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
            .unwrap();
        cx.run_until_parked();
        let (view_commands, view_inbox) = mpsc::unbounded_channel();
        let window = cx.update(|cx| {
            let model = model.clone();
            cx.open_window(gpui::WindowOptions::default(), |_, cx| {
                cx.new(|cx| ScreenView::new(model, view_commands.clone(), cx))
            })
            .expect("a window")
        });
        cx.simulate_window_resize(window.into(), gpui::size(px(size.0 - SIDEBAR), px(size.1)));
        window
            .update(cx, |view, _, cx| {
                view.show_leakshield_tab(LEAKSHIELD, tab, cx);
            })
            .unwrap();
        Rig {
            window,
            model,
            events,
            _keep: Box::new((runtime, commands, view_inbox)),
        }
    }

    fn draw(rig: &Rig, cx: &mut TestAppContext) {
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(rig.window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(rig.window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn bounds(rig: &Rig, cx: &mut TestAppContext, name: String) -> Option<Bounds<Pixels>> {
        let mut visual = VisualTestContext::from_window(rig.window.into(), cx);
        visual.debug_bounds(Box::leak(name.into_boxed_str()))
    }

    const CASES: [(Language, (f32, f32)); 6] = [
        (Language::English, (1100., 700.)),
        (Language::English, (1280., 800.)),
        (Language::Italian, (1100., 700.)),
        (Language::Italian, (1280., 800.)),
        (Language::German, (1100., 700.)),
        (Language::German, (1280., 800.)),
    ];

    /// Every template: the tiles, the preview and the last control of each row end inside the
    /// card, and the page is not wider than the window.
    #[gpui::test]
    fn the_display_tab_fits_each_template_at_both_sizes_in_both_languages(cx: &mut TestAppContext) {
        for (language, size) in CASES {
            let rig = open(cx, language, size, Tab::Display);
            let tag = format!("{language:?} {size:?}");
            for (template, slots) in LAYOUTS
                .iter()
                .copied()
                .chain([(NO_TEMPLATE, Default::default())])
            {
                let mut state = with_leakshield();
                let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
                templates[0] = template.to_owned();
                super::set_field(&mut state, "page_template", FieldValue::TextList(templates));
                // Each value on Software sensor 12 without a source: the longest source name
                // and the warning row.
                super::set_field(&mut state, "page_sources", FieldValue::List(vec![76.0; 80]));
                // An icon in each slot, so that each icon row draws its image.
                super::set_field(&mut state, "page_icons", FieldValue::List(vec![3.0; 40]));
                rig.events
                    .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
                    .unwrap();
                cx.run_until_parked();
                cx.update(|cx| {
                    rig.model.update(cx, |app, cx| {
                        let mut image = Bitmap::new(128, 64);
                        image.set(0, 0, true);
                        app.state
                            .display_previews
                            .insert((LEAKSHIELD.to_owned(), 0), PagePreview::Image(image));
                        // Page 2 has the longest reason of a preview without an image.
                        app.state.display_previews.insert(
                            (LEAKSHIELD.to_owned(), 1),
                            PagePreview::Failed("ReadOnly".into()),
                        );
                        cx.notify();
                    });
                });
                for selected in [0usize, 1] {
                    rig.window
                        .update(cx, |view, _, cx| {
                            view.leakshield_ui().display.select(selected);
                            cx.notify();
                        })
                        .unwrap();
                    draw(&rig, cx);
                    let tag = format!("{tag} {template} page {selected}");
                    let page = bounds(&rig, cx, "screen-page".into()).expect("the page");
                    assert!(
                        page.size.width <= px(size.0 - SIDEBAR + 0.5),
                        "{tag}: the page is {:?} wide",
                        page.size.width
                    );
                    let card_end = page.right() - px(24.);
                    let row_end = card_end - px(18.);
                    for index in 0..10 {
                        let tile = bounds(&rig, cx, format!("ls-page-{index}"))
                            .unwrap_or_else(|| panic!("{tag}: no tile {index}"));
                        assert!(
                            tile.right() <= card_end - px(12.) + px(0.5),
                            "{tag}: tile {index} ends at {:?}, the card at {card_end:?}",
                            tile.right()
                        );
                    }
                    // The tiles are 5 in a row: tile 6 is under tile 1.
                    let first = bounds(&rig, cx, "ls-page-0".into()).unwrap();
                    let sixth = bounds(&rig, cx, "ls-page-5".into()).unwrap();
                    assert!(
                        sixth.top() >= first.bottom() && sixth.left() == first.left(),
                        "{tag}"
                    );
                    let panel = bounds(&rig, cx, "display-preview-panel".into()).expect("panel");
                    assert!(panel.left() >= page.left() + px(24.), "{tag}");
                    let change =
                        bounds(&rig, cx, "display-template-change".into()).expect("change");
                    assert!(change.right() <= row_end + px(0.5), "{tag}: {change:?}");
                    assert!(
                        change.left() >= panel.right(),
                        "{tag}: the button is in the preview"
                    );
                    if selected != 0 {
                        continue;
                    }
                    let mut ends = Vec::new();
                    for slot in 0..slots.values {
                        ends.push(format!("display-decimals-{slot}"));
                        if slots.ranges > 0 {
                            ends.push(format!("display-max-{slot}"));
                        }
                    }
                    ends.extend((0..slots.texts).map(|slot| format!("display-own-text-{slot}")));
                    ends.extend((0..slots.icons).map(|slot| format!("display-change-icon-{slot}")));
                    if slots.logo {
                        ends.push("display-change-logo".into());
                    }
                    for name in ends {
                        let control = bounds(&rig, cx, name.clone())
                            .unwrap_or_else(|| panic!("{tag}: no {name}"));
                        assert!(
                            control.right() <= row_end + px(0.5) && control.size.width > px(20.),
                            "{tag}: {name} is {control:?}, the row ends at {row_end:?}"
                        );
                    }
                    // The data source keeps a usable width next to the other cells.
                    if slots.values > 0 {
                        let decimals = bounds(&rig, cx, "display-decimals-0".into()).unwrap();
                        assert!(
                            decimals.left() - page.left() >= px(24. + 18. + 90. + 12. + 200.),
                            "{tag}: the data source is narrow: decimals start at {:?}",
                            decimals.left()
                        );
                    }
                }
            }
            cx.update(|cx| cx.quit());
        }
    }

    /// Each dialog of the tab is inside the window.
    #[gpui::test]
    fn the_dialogs_of_the_display_tab_fit_the_window(cx: &mut TestAppContext) {
        for (language, size) in CASES {
            let rig = open(cx, language, size, Tab::Display);
            let tag = format!("{language:?} {size:?}");
            let logo = import_png(&png(640, 480, |_, _| WHITE), 128, 64);
            let icon = import_png(&png(64, 64, |_, _| WHITE), 23, 23);
            type Open = Box<dyn Fn(&mut nicecooling::screens::leakshield::display::DisplayUi)>;
            let dialogs: Vec<(&str, Open)> = vec![
                ("gallery", Box::new(|ui| ui.gallery = true)),
                (
                    "own text",
                    Box::new(|ui| {
                        ui.open_own_text(0);
                        ui.type_own_text("A text of 24 characters.");
                    }),
                ),
                ("icons", Box::new(|ui| ui.open_icons(0))),
                (
                    "logo import",
                    Box::new(move |ui| {
                        ui.open_import(ImportTarget::Logo);
                        ui.import_loaded("a-long-file-name-of-a-logo.png".into(), logo.clone());
                    }),
                ),
                (
                    "icon import",
                    Box::new(move |ui| {
                        ui.open_import(ImportTarget::Icon { slot: 0 });
                        ui.import_loaded("icon.png".into(), icon.clone());
                    }),
                ),
            ];
            for (name, open_dialog) in &dialogs {
                rig.window
                    .update(cx, |view, _, cx| {
                        let ui = &mut view.leakshield_ui().display;
                        // Page 3 of the fixture has "2 values": texts and icons.
                        ui.select(2);
                        ui.close_dialog();
                        open_dialog(ui);
                        cx.notify();
                    })
                    .unwrap();
                draw(&rig, cx);
                let dialog = bounds(&rig, cx, "modal-dialog".into())
                    .unwrap_or_else(|| panic!("{tag}: no dialog for {name}"));
                assert!(
                    dialog.top() >= px(0.) && dialog.bottom() <= px(size.1),
                    "{tag}: the {name} dialog is {dialog:?} in a window of {size:?}"
                );
                assert!(
                    dialog.left() >= px(0.) && dialog.right() <= px(size.0 - SIDEBAR),
                    "{tag}: the {name} dialog is {dialog:?}"
                );
                if name.ends_with("import") {
                    let result = bounds(&rig, cx, "display-import-result".into())
                        .unwrap_or_else(|| panic!("{tag}: no result for {name}"));
                    assert!(
                        result.left() >= dialog.left() + px(18.)
                            && result.right() <= dialog.right() - px(18.)
                            && result.bottom() < dialog.bottom(),
                        "{tag}: the result of the {name} is {result:?} in {dialog:?}"
                    );
                    // The logo shows at 2 px for each pixel, the icon at 4 px, plus the frame.
                    let expected = if *name == "logo import" {
                        (264., 136.)
                    } else {
                        (100., 100.)
                    };
                    assert_eq!(
                        (result.size.width, result.size.height),
                        (px(expected.0), px(expected.1)),
                        "{tag}: {name}"
                    );
                }
                if *name == "gallery" {
                    for (template, _) in LAYOUTS
                        .iter()
                        .copied()
                        .chain([(NO_TEMPLATE, Default::default())])
                    {
                        let tile = bounds(&rig, cx, format!("ls-gallery-{template}"))
                            .unwrap_or_else(|| panic!("{tag}: no gallery tile {template}"));
                        assert!(
                            tile.right() <= dialog.right() - px(18.) + px(0.5)
                                && tile.bottom() <= dialog.bottom(),
                            "{tag}: the tile {template} is {tile:?} in {dialog:?}"
                        );
                    }
                }
            }
            cx.update(|cx| cx.quit());
        }
    }

    /// A preview draws at a whole scale: 128 x 64 in the grid and 256 x 128 on the page, with
    /// the frame of 4 px.
    #[gpui::test]
    fn a_preview_has_a_whole_scale(cx: &mut TestAppContext) {
        let rig = open(cx, Language::English, (1100., 700.), Tab::Display);
        cx.update(|cx| {
            rig.model.update(cx, |app, cx| {
                for page in 0..3 {
                    app.state.display_previews.insert(
                        (LEAKSHIELD.to_owned(), page),
                        PagePreview::Image(Bitmap::new(128, 64)),
                    );
                }
                cx.notify();
            });
        });
        draw(&rig, cx);
        // The last "display-screen" that draws is the large one of the selected page.
        let large = bounds(&rig, cx, "display-screen".into()).expect("the large preview");
        assert_eq!(
            (large.size.width, large.size.height),
            (px(264.), px(136.)),
            "2 px for each pixel, and 4 px of frame at each side"
        );
        let tile = bounds(&rig, cx, "ls-page-0".into()).expect("a tile");
        // The tile has 3 px of padding and 2 px of border around its preview of 136 px.
        assert_eq!(tile.size.width, px(136. + 10.));
        cx.update(|cx| cx.quit());
    }

    /// Enter in an input of the page card commits the input. It does not open the template
    /// gallery, and the arrow keys do not move the page selection.
    #[gpui::test]
    fn the_keys_of_an_input_stay_in_the_input(cx: &mut TestAppContext) {
        let rig = open(cx, Language::English, (1280., 800.), Tab::Display);
        let mut state = with_leakshield();
        let mut templates = vec![NO_TEMPLATE.to_owned(); 10];
        templates[0] = "bars-2".into();
        super::set_field(&mut state, "page_template", FieldValue::TextList(templates));
        rig.events
            .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
            .unwrap();
        draw(&rig, cx);
        let input = bounds(&rig, cx, "display-max-0".into()).expect("the maximum");
        let mut visual = VisualTestContext::from_window(rig.window.into(), cx);
        visual.simulate_click(input.center(), gpui::Modifiers::none());
        visual.simulate_keystrokes("ctrl-a");
        visual.simulate_input("450");
        visual.simulate_keystrokes("enter right down");
        visual.run_until_parked();
        let (gallery, page) = rig
            .window
            .update(cx, |view, _, _| {
                let ui = &view.leakshield_ui().display;
                (ui.gallery, ui.page)
            })
            .unwrap();
        assert!(!gallery, "Enter in an input opens no gallery");
        assert_eq!(page, 0, "the arrow keys stay in the input");
        cx.update(|cx| cx.quit());
    }

    /// The Sensors, System, Status and Overview tabs with their new controls.
    #[gpui::test]
    fn the_other_tabs_fit_at_both_sizes_in_both_languages(cx: &mut TestAppContext) {
        for (language, size) in CASES {
            let tag = format!("{language:?} {size:?}");
            let rig = open(cx, language, size, Tab::Sensors);
            draw(&rig, cx);
            let page = bounds(&rig, cx, "screen-page".into()).expect("the page");
            let row_end = page.right() - px(24. + 18.);
            assert!(page.size.width <= px(size.0 - SIDEBAR + 0.5), "{tag}");
            for slot in 1..=12 {
                let value = bounds(&rig, cx, format!("software-value-{slot}"))
                    .unwrap_or_else(|| panic!("{tag}: no value of slot {slot}"));
                assert!(
                    value.right() <= row_end + px(0.5),
                    "{tag}: the value of slot {slot} ends at {:?}, the row at {row_end:?}",
                    value.right()
                );
            }
            // Slot 3 has a source: its "Clear" is the last control of the row.
            let clear = bounds(&rig, cx, "software-clear-3".into()).expect("Clear of slot 3");
            assert!(
                clear.right() <= row_end + px(0.5),
                "{tag}: {clear:?} ends after {row_end:?}"
            );

            rig.window
                .update(cx, |view, _, cx| {
                    view.show_leakshield_tab(LEAKSHIELD, Tab::System, cx)
                })
                .unwrap();
            draw(&rig, cx);
            let value = bounds(&rig, cx, "device-state-value".into()).expect("a counter");
            assert!(value.right() <= row_end + px(0.5), "{tag}: {value:?}");
            assert!(
                value.left() >= page.left() + px(24. + 18. + 200.),
                "{tag}: {value:?}"
            );

            rig.window
                .update(cx, |view, _, cx| {
                    view.show_leakshield_tab(LEAKSHIELD, Tab::Status, cx)
                })
                .unwrap();
            draw(&rig, cx);
            let export = bounds(&rig, cx, "status-export".into()).expect("Export");
            assert!(
                export.right() <= page.right() - px(24. + 12.) + px(0.5),
                "{tag}: {export:?}"
            );
            rig.window
                .update(cx, |view, _, cx| {
                    view.leakshield_ui().status.request_reset();
                    cx.notify();
                })
                .unwrap();
            draw(&rig, cx);
            let dialog = bounds(&rig, cx, "modal-dialog".into()).expect("the question");
            assert!(dialog.bottom() <= px(size.1), "{tag}: {dialog:?}");
            cx.update(|cx| cx.quit());
        }
    }
}
