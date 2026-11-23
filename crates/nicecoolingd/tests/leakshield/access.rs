use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::feed::FeedMessage;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;

use crate::Rig;

#[test]
fn report1_read_uses_read_not_hidiocginput() {
    let rig = Rig::new("report1");
    let report1 = rig.io.read_report1().unwrap();
    assert_eq!(report1.firmware_version, 1022);
    // The transport has no call for HIDIOCGINPUT, so the only request is the `read()`.
    assert_eq!(rig.sim.requests(), [Request::ReadInput]);
}

#[test]
fn write_report3_with_crc() {
    let rig = Rig::new("write3");
    let mut report = rig.io.read_report3().unwrap();
    let crc_before = report.crc();
    report.set_bytes(63, &150u16.to_be_bytes());
    rig.sim.clear_requests();
    rig.io.write_report3(&report).unwrap();
    assert_eq!(
        rig.sim.requests(),
        [Request::SetFeature { id: 3, len: 750 }]
    );
    let stored = rig.sim.report3();
    // The device only accepts a report with a valid CRC, and the CRC follows the change.
    let decoded = Report3::decode(&stored).unwrap();
    assert_eq!(decoded, report);
    assert_ne!(decoded.crc(), crc_before);
    assert_eq!(&stored[748..], &decoded.crc().to_be_bytes());
}

#[test]
fn detach_closes_the_transports_and_replace_opens_new_ones() {
    let rig = Rig::new("swap");
    assert!(rig.io.read_report1().is_ok());

    rig.io.detach();
    assert!(matches!(
        rig.io.read_report1(),
        Err(DeviceError::Disconnected)
    ));
    assert!(matches!(
        rig.io.send_feed(&FeedMessage::default()),
        Err(DeviceError::Disconnected)
    ));

    // The clones of the handle share the transports, so a replug reaches the write worker and
    // the Feed sender too.
    let other = Rig::new("swap-new");
    let shared = rig.io.clone();
    rig.io.replace(&other.io);
    assert!(shared.read_report1().is_ok());
    shared.send_feed(&FeedMessage::default()).unwrap();
    assert_eq!(other.sim.bulk_messages().len(), 1);
    assert!(rig.sim.bulk_messages().is_empty());
}

#[test]
fn bulk_write_failure_reported() {
    let rig = Rig::new("bulk");
    rig.sim.fail_bulk(true);
    assert!(matches!(
        rig.io.send_feed(&FeedMessage::default()),
        Err(DeviceError::Io(_))
    ));
    assert!(rig.sim.bulk_messages().is_empty());
    rig.sim.fail_bulk(false);
    rig.io.send_feed(&FeedMessage::default()).unwrap();
    assert_eq!(rig.sim.bulk_messages().len(), 1);
}
