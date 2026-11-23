use crc::{CRC_16_USB, Crc};

static USB: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);

/// CRC-16/USB as every Leakshield report uses it (init 0xFFFF, reflected, final XOR 0xFFFF).
pub fn crc16_usb(data: &[u8]) -> u16 {
    USB.checksum(data)
}
