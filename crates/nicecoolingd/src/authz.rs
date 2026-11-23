//! Authorization of method calls. The daemon asks polkit through `Authorizer`. A caller with
//! uid 0 needs no check. The release binary builds `PolkitAuthorizer` only.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use nicecooling_api::Error;
use nicecooling_api::authz_map::{Action, AuthzMap};
use zbus::Connection;
use zbus::message::{Flags, Header};
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One question to the authority.
pub struct Request<'a> {
    pub header: &'a Header<'a>,
    pub uid: u32,
    pub action: Action,
    /// The method call carries the D-Bus flag `ALLOW_INTERACTIVE_AUTHORIZATION`.
    pub interactive: bool,
}

pub trait Authorizer: Send + Sync + 'static {
    /// `Ok(true)` when the caller may run the action.
    fn check<'a>(&'a self, request: Request<'a>) -> BoxFuture<'a, Result<bool, Error>>;
}

/// Asks `org.freedesktop.PolicyKit1` on the bus of the connection.
pub struct PolkitAuthorizer {
    authority: AuthorityProxy<'static>,
}

impl PolkitAuthorizer {
    pub async fn new(connection: &Connection) -> zbus::Result<Self> {
        Ok(Self {
            authority: AuthorityProxy::new(connection).await?,
        })
    }
}

impl Authorizer for PolkitAuthorizer {
    fn check<'a>(&'a self, request: Request<'a>) -> BoxFuture<'a, Result<bool, Error>> {
        Box::pin(async move {
            let denied = |e: &dyn std::fmt::Display| Error::access_denied(format!("polkit: {e}"));
            let subject =
                Subject::new_for_message_header(request.header).map_err(|e| denied(&e))?;
            let flags = if request.interactive {
                CheckAuthorizationFlags::ALLOW_USER_INTERACTION
            } else {
                CheckAuthorizationFlags::empty()
            };
            let result = self
                .authority
                .check_authorization(
                    &subject,
                    request.action.id(),
                    &std::collections::HashMap::new(),
                    flags,
                    "",
                )
                .await
                .map_err(|e| denied(&e))?;
            Ok(result.is_authorized)
        })
    }
}

/// What a method asks, and how a caller is told apart.
pub struct Authz {
    map: &'static AuthzMap,
    authorizer: Arc<dyn Authorizer>,
}

impl Authz {
    pub fn new(authorizer: Arc<dyn Authorizer>) -> Self {
        Self {
            map: AuthzMap::embedded(),
            authorizer,
        }
    }

    /// Checks the caller of a method. `interface` is the short name (`Channel`). `mode` is the
    /// first argument of `SetMode`.
    pub async fn check(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        interface: &str,
        method: &str,
        mode: Option<&str>,
    ) -> Result<(), Error> {
        let Some(authorization) = self.map.get(interface, method) else {
            return Err(Error::ZBus(
                zbus::fdo::Error::Failed(format!("{interface}.{method} has no authorization"))
                    .into(),
            ));
        };
        let Some(action) = authorization.action(mode) else {
            return Ok(());
        };
        let uid = caller_uid(connection, header).await?;
        self.decide(header, uid, action).await
    }

    /// The decision for a known caller. A caller with uid 0 needs no polkit check.
    pub async fn decide(&self, header: &Header<'_>, uid: u32, action: Action) -> Result<(), Error> {
        if uid == 0 {
            return Ok(());
        }
        let interactive = header
            .primary()
            .flags()
            .contains(Flags::ALLOW_INTERACTIVE_AUTH);
        let allowed = self
            .authorizer
            .check(Request {
                header,
                uid,
                action,
                interactive,
            })
            .await?;
        if allowed {
            Ok(())
        } else {
            Err(Error::access_denied(format!(
                "not authorized for {}",
                action.id()
            )))
        }
    }
}

async fn caller_uid(connection: &Connection, header: &Header<'_>) -> Result<u32, Error> {
    let sender = header
        .sender()
        .ok_or_else(|| Error::access_denied("the call has no sender"))?;
    let bus = zbus::fdo::DBusProxy::new(connection).await?;
    bus.get_connection_unix_user(sender.clone().into())
        .await
        .map_err(|e| Error::access_denied(format!("the caller uid is unknown: {e}")))
}

#[cfg(any(test, feature = "test-support"))]
pub use test_support::TestAuthorizer;

#[cfg(any(test, feature = "test-support"))]
mod test_support {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use super::*;

    /// Allows every action except the denied ones, and records each question.
    #[derive(Default)]
    pub struct TestAuthorizer {
        denied: Mutex<BTreeSet<Action>>,
        calls: Mutex<Vec<(Action, bool)>>,
    }

    impl TestAuthorizer {
        pub fn allow_all() -> Arc<Self> {
            Arc::default()
        }

        pub fn deny(&self, action: Action) {
            self.denied.lock().unwrap().insert(action);
        }

        pub fn allow(&self, action: Action) {
            self.denied.lock().unwrap().remove(&action);
        }

        /// The questions so far: the action and the interactive flag.
        pub fn calls(&self) -> Vec<(Action, bool)> {
            self.calls.lock().unwrap().clone()
        }

        pub fn clear(&self) {
            self.calls.lock().unwrap().clear();
        }
    }

    impl Authorizer for TestAuthorizer {
        fn check<'a>(&'a self, request: Request<'a>) -> BoxFuture<'a, Result<bool, Error>> {
            self.calls
                .lock()
                .unwrap()
                .push((request.action, request.interactive));
            let allowed = !self.denied.lock().unwrap().contains(&request.action);
            Box::pin(async move { Ok(allowed) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::message::Message;

    fn message(flags: Option<Flags>) -> Message {
        let mut builder = Message::method_call("/", "Ping").destination("a.b");
        if let Some(flags) = flags {
            builder = builder.with_flags(flags);
        }
        builder.build(&()).unwrap()
    }

    #[tokio::test]
    async fn uid_0_needs_no_check() {
        let authorizer = TestAuthorizer::allow_all();
        authorizer.deny(Action::Critical);
        let authz = Authz::new(authorizer.clone());
        let message = message(None);
        let header = message.header();
        authz.decide(&header, 0, Action::Critical).await.unwrap();
        assert!(authorizer.calls().is_empty());
        let denied = authz
            .decide(&header, 1000, Action::Critical)
            .await
            .unwrap_err();
        assert_eq!(
            denied.dbus_name(),
            "org.freedesktop.DBus.Error.AccessDenied"
        );
        assert_eq!(authorizer.calls(), [(Action::Critical, false)]);
    }
}
