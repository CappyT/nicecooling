//! Big-endian cursor over a report. The cursor position equals the report offset.

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub(crate) fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let out = self.buf[self.pos..self.pos + N]
            .try_into()
            .expect("slice has N bytes");
        self.pos += N;
        out
    }

    pub(crate) fn u8(&mut self) -> u8 {
        self.bytes::<1>()[0]
    }

    pub(crate) fn u16(&mut self) -> u16 {
        u16::from_be_bytes(self.bytes())
    }

    pub(crate) fn i16(&mut self) -> i16 {
        i16::from_be_bytes(self.bytes())
    }

    pub(crate) fn u32(&mut self) -> u32 {
        u32::from_be_bytes(self.bytes())
    }

    pub(crate) fn i32(&mut self) -> i32 {
        i32::from_be_bytes(self.bytes())
    }

    pub(crate) fn array<T: Default + Copy, const N: usize>(
        &mut self,
        mut read: impl FnMut(&mut Self) -> T,
    ) -> [T; N] {
        let mut out = [T::default(); N];
        for slot in &mut out {
            *slot = read(self);
        }
        out
    }
}

pub(crate) struct Writer(pub(crate) Vec<u8>);

impl Writer {
    pub(crate) fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    pub(crate) fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
}
