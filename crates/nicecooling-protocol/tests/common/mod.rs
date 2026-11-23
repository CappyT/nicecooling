#![allow(dead_code)]

pub struct Fixtures {
    pub report1: &'static [u8],
    pub report3: &'static [u8],
    pub report5: &'static [u8],
    pub report8: &'static [u8],
}

macro_rules! fixtures {
    ($dir:literal) => {
        Fixtures {
            report1: include_bytes!(concat!(
                "../../../../tests/fixtures/leakshield/",
                $dir,
                "/report1.bin"
            )),
            report3: include_bytes!(concat!(
                "../../../../tests/fixtures/leakshield/",
                $dir,
                "/report3.bin"
            )),
            report5: include_bytes!(concat!(
                "../../../../tests/fixtures/leakshield/",
                $dir,
                "/report5.bin"
            )),
            report8: include_bytes!(concat!(
                "../../../../tests/fixtures/leakshield/",
                $dir,
                "/report8.bin"
            )),
        }
    };
}

pub fn fw1019() -> Fixtures {
    fixtures!("fw1019")
}

pub fn fw1022() -> Fixtures {
    fixtures!("fw1022")
}
