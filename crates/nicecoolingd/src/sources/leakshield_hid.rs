//! The HID interface of the Leakshield through `/dev/hidraw*`. The daemon reads report 1 with
//! `read()` of the input reports and never asks for it with `HIDIOCGINPUT`.

use std::ffi::c_void;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nicecooling_protocol::device::{DeviceError, HidTransport};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{Mode, OFlags, open};
use rustix::io::Errno;
use rustix::ioctl::{Ioctl, IoctlOutput, Opcode};

/// Interface 1 of the Leakshield is the HID interface (`docs/protocol/leakshield.md`).
pub const HID_INTERFACE: u8 = 1;

const HIDIOC_GROUP: u32 = b'H' as u32;
const HIDIOCSFEATURE: u32 = 0x06;
const HIDIOCGFEATURE: u32 = 0x07;

/// `_IOC(_IOC_READ | _IOC_WRITE, 'H', number, len)`: the 2 direction bits, 14 size bits, 8 group
/// bits and 8 number bits of the Linux ioctl encoding.
const fn feature_opcode(number: u32, len: usize) -> Opcode {
    (3 << 30) | ((len as u32) << 16) | (HIDIOC_GROUP << 8) | number
}

/// The name of the hidraw node of the Leakshield at `usb_path`, from sysfs: `hidraw3`. Both roots
/// are parameters, so tests use a fake tree.
pub fn find_hidraw(sysfs_root: &Path, usb_path: &str) -> Option<String> {
    let dir = sysfs_root
        .join("bus/usb/devices")
        .join(usb_path)
        .join(format!("{usb_path}:1.{HID_INTERFACE}"))
        .join("hidraw");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("hidraw"))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// `HIDIOCSFEATURE(len)` or `HIDIOCGFEATURE(len)` over a buffer whose first byte is the report ID.
struct Feature<'a> {
    number: u32,
    buf: &'a mut [u8],
}

// SAFETY: the opcode carries the exact length of `buf`, and `as_ptr` points at `buf`.
unsafe impl Ioctl for Feature<'_> {
    type Output = usize;
    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        feature_opcode(self.number, self.buf.len())
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.buf.as_mut_ptr().cast()
    }

    unsafe fn output_from_ptr(out: IoctlOutput, _: *mut c_void) -> rustix::io::Result<usize> {
        Ok(out as usize)
    }
}

fn map_errno(errno: Errno) -> DeviceError {
    // A hidraw node of an unplugged device answers with these errors.
    match errno {
        Errno::NODEV | Errno::NXIO | Errno::SHUTDOWN | Errno::NOTCONN | Errno::NOENT => {
            DeviceError::Disconnected
        }
        errno => DeviceError::Io(io::Error::from(errno)),
    }
}

pub struct Hidraw {
    fd: OwnedFd,
}

impl Hidraw {
    pub fn open(path: &Path) -> Result<Self, DeviceError> {
        let fd = open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()).map_err(map_errno)?;
        Ok(Self { fd })
    }

    /// Node of a hidraw name from `find_hidraw`, below `dev_root` (`/dev` in the daemon).
    pub fn node(dev_root: &Path, name: &str) -> PathBuf {
        dev_root.join(name)
    }

    fn feature(&self, number: u32, buf: &mut [u8]) -> Result<usize, DeviceError> {
        // SAFETY: `Feature` passes the length of `buf` in the opcode.
        unsafe { rustix::ioctl::ioctl(&self.fd, Feature { number, buf }) }.map_err(map_errno)
    }
}

impl HidTransport for Hidraw {
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        let timeout = Timespec {
            tv_sec: timeout.as_secs() as _,
            tv_nsec: timeout.subsec_nanos() as _,
        };
        let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
        if poll(&mut fds, Some(&timeout)).map_err(map_errno)? == 0 {
            return Err(DeviceError::Timeout);
        }
        if fds[0]
            .revents()
            .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
        {
            return Err(DeviceError::Disconnected);
        }
        let mut buf = vec![0; 1024];
        // Reading an unplugged hidraw node gives EIO.
        let n = rustix::io::read(&self.fd, &mut buf[..]).map_err(|e| match e {
            Errno::IO => DeviceError::Disconnected,
            e => map_errno(e),
        })?;
        buf.truncate(n);
        Ok(buf)
    }

    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        let mut buf = vec![0; len];
        buf[0] = id;
        let n = self.feature(HIDIOCGFEATURE, &mut buf)?;
        if n != len {
            return Err(DeviceError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("feature report {id}: {n} bytes instead of {len}"),
            )));
        }
        Ok(buf)
    }

    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        let mut buf = report.to_vec();
        self.feature(HIDIOCSFEATURE, &mut buf).map(|_| ())
    }

    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        match rustix::io::write(&self.fd, report).map_err(map_errno)? {
            n if n == report.len() => Ok(()),
            n => Err(DeviceError::Io(io::Error::new(
                io::ErrorKind::WriteZero,
                format!("output report: {n} of {} bytes", report.len()),
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn feature_opcodes_follow_the_kernel_macros() {
        // HIDIOCGFEATURE(256) is 0xC1004807 and HIDIOCSFEATURE(256) is 0xC1004806 in hidraw.h.
        let get = |len| feature_opcode(HIDIOCGFEATURE, len);
        let set = |len| feature_opcode(HIDIOCSFEATURE, len);
        assert_eq!(get(256), 0xC100_4807);
        assert_eq!(set(256), 0xC100_4806);
        assert_eq!(get(750), 0xC2EE_4807);
    }

    #[test]
    fn hidraw_name_from_a_fake_sysfs() {
        let root = std::env::temp_dir().join(format!("nicecooling-hid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let hid = root.join("bus/usb/devices/1-2.3/1-2.3:1.1/hidraw/hidraw7");
        fs::create_dir_all(&hid).unwrap();
        fs::create_dir_all(root.join("bus/usb/devices/1-2.3/1-2.3:1.2/hidraw/hidraw8")).unwrap();
        assert_eq!(find_hidraw(&root, "1-2.3").as_deref(), Some("hidraw7"));
        assert_eq!(find_hidraw(&root, "1-4"), None);
        assert_eq!(
            Hidraw::node(Path::new("/tmp/dev"), "hidraw7"),
            Path::new("/tmp/dev/hidraw7")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
