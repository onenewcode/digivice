use digivice_core::domain::HidAction;
use digivice_core::protocol::{
    CONSUMER_REPORT_ID, CONSUMER_REPORT_MAP, CONSUMER_REPORT_REFERENCE, ConsumerControlReport,
};

#[test]
fn gatt_input_values_contain_only_the_sixteen_bit_usage() {
    for (action, expected) in [
        (HidAction::PlayPause, [0xCD, 0x00]),
        (HidAction::PreviousTrack, [0xB6, 0x00]),
        (HidAction::NextTrack, [0xB5, 0x00]),
        (HidAction::VolumeUp, [0xE9, 0x00]),
        (HidAction::VolumeDown, [0xEA, 0x00]),
        (HidAction::Mute, [0xE2, 0x00]),
    ] {
        let bytes = ConsumerControlReport::from_action(action).bytes();
        assert_eq!(bytes.as_slice(), expected, "action: {action:?}");
    }
}

#[test]
fn gatt_release_is_a_two_byte_null_usage_without_a_report_id() {
    assert_eq!(ConsumerControlReport::release_bytes().as_slice(), [0, 0]);
}

#[test]
fn report_map_and_reference_describe_exactly_one_sixteen_bit_input() {
    let mut offset = 0;
    let mut report_id = 0;
    let mut report_size = 0;
    let mut report_count = 0;
    let mut inputs = 0;
    while offset < CONSUMER_REPORT_MAP.len() {
        let prefix = CONSUMER_REPORT_MAP[offset];
        assert_ne!(prefix, 0xFE, "unexpected long HID item");
        let size = match prefix & 3 {
            3 => 4,
            size => size as usize,
        };
        let data = &CONSUMER_REPORT_MAP[offset + 1..offset + 1 + size];
        match prefix {
            0x85 => report_id = data[0],
            0x75 => report_size = data[0],
            0x95 => report_count = data[0],
            0x81 => {
                assert_eq!(report_id, CONSUMER_REPORT_ID);
                assert_eq!(report_size, 16);
                assert_eq!(report_count, 1);
                assert_eq!(data, [0], "Consumer usages form one array field");
                inputs += 1;
            }
            0x91 | 0xB1 => panic!("unexpected Output or Feature Report"),
            _ => {}
        }
        offset += 1 + size;
    }
    assert_eq!(inputs, 1);
    assert_eq!(CONSUMER_REPORT_REFERENCE, [report_id, 1]);
    assert_eq!(
        usize::from(report_size) * usize::from(report_count),
        ConsumerControlReport::release_bytes().len() * 8
    );
}
