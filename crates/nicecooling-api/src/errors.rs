//! The errors of `docs/api.md`. The names have the prefix `io.github.cappyt.NiceCooling1.Error.`.
//! polkit denials and `NotSupported` use the `org.freedesktop.DBus.Error` names.

use zbus::DBusError;
use zbus::fdo;

#[derive(Debug, DBusError)]
#[zbus(prefix = "io.github.cappyt.NiceCooling1.Error")]
pub enum Error {
    /// Also carries the errors of the bus, among them `AccessDenied` and `NotSupported`.
    #[zbus(error)]
    ZBus(zbus::Error),
    InvalidArgument(String),
    NotFound(String),
    InUse(String),
    DefaultProfile(String),
    ReadOnly(String),
    NotVerified(String),
    PumpRunning(String),
    Busy(String),
    DeviceChanged(String),
    VerifyFailed(String),
    Disconnected(String),
    NoPoweroff(String),
    PoweroffStarted(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn in_use(message: impl Into<String>) -> Self {
        Self::InUse(message.into())
    }

    pub fn read_only(reason: impl Into<String>) -> Self {
        Self::ReadOnly(reason.into())
    }

    /// `org.freedesktop.DBus.Error.NotSupported`.
    pub fn not_supported(message: impl Into<String>) -> Self {
        Self::ZBus(fdo::Error::NotSupported(message.into()).into())
    }

    /// `org.freedesktop.DBus.Error.AccessDenied`, the reply to a polkit denial.
    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::ZBus(fdo::Error::AccessDenied(message.into()).into())
    }

    /// The full D-Bus name of the error.
    pub fn dbus_name(&self) -> String {
        DBusError::name(self).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_api_md() {
        assert_eq!(
            Error::invalid("x").dbus_name(),
            "io.github.cappyt.NiceCooling1.Error.InvalidArgument"
        );
        assert_eq!(
            Error::in_use("x").dbus_name(),
            "io.github.cappyt.NiceCooling1.Error.InUse"
        );
        assert_eq!(
            Error::not_supported("x").dbus_name(),
            "org.freedesktop.DBus.Error.NotSupported"
        );
        assert_eq!(
            Error::access_denied("x").dbus_name(),
            "org.freedesktop.DBus.Error.AccessDenied"
        );
    }
}
