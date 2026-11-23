mod common;

use common::fw1022;
use nicecooling_protocol::lighting::{Binding, read_lighting};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;

fn report3() -> Report3 {
    Report3::decode(fw1022().report3).unwrap()
}

#[test]
fn read_lighting_from_fixture() {
    let registry = Registry::leakshield();
    let report = report3();
    let lighting = read_lighting(&registry, &report).unwrap();

    assert_eq!(lighting.strip.brightness, 255);
    assert_eq!(lighting.strip.flags, 4);
    assert_eq!(lighting.controllers.len(), 2);
    let [first, second] = &lighting.controllers[..] else {
        unreachable!()
    };
    assert_eq!((first.first_led, first.led_count, first.mode), (0, 6, 3));
    assert_eq!(first.source, -1);
    assert_eq!(first.source_filter, [10, 15]);
    assert_eq!(
        first.bindings[0],
        Binding {
            x1: 0,
            x2: 100,
            y1: 1,
            y2: 100
        }
    );
    assert_eq!(
        first.bindings[1],
        Binding {
            x1: 0,
            x2: 100,
            y1: 0,
            y2: 255
        }
    );
    assert_eq!(first.colors[0].saturation, 255);
    assert_eq!(first.colors[0].value, 255);
    assert_eq!((second.mode, second.source_filter), (0, [15, 25]));
    assert_eq!(second.bindings[0].x2, 1000);
    // Each controller comes from its own 70 bytes.
    for (i, controller) in lighting.controllers.iter().enumerate() {
        let hue = |c: usize| {
            let at = 620 + 70 * i + 4 * c;
            u16::from_be_bytes([report.bytes_at(at, 2)[0], report.bytes_at(at, 2)[1]])
        };
        let colors: Vec<u16> = controller.colors.iter().map(|c| c.hue).collect();
        assert_eq!(colors, (0..6).map(hue).collect::<Vec<_>>());
        assert_eq!(controller.parameters.len(), 12);
    }
}
