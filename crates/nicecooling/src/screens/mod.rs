//! The screens of `docs/ui/screens/`. Each screen has a model that decides what shows and
//! a view that draws it. The tests read the models.

pub mod alarms;
pub mod alarms_view;
pub mod channels;
pub mod channels_view;
pub(crate) mod controls;
pub mod countdown_view;
pub mod first_run;
pub mod first_run_view;
pub mod format;
mod handlers;
pub mod history;
pub mod history_view;
pub mod leakshield;
pub mod loops;
pub mod loops_view;
pub mod overview;
pub mod overview_view;
pub mod profiles;
pub mod profiles_view;
pub mod sensors;
pub mod sensors_view;
pub mod settings;
pub mod settings_view;
pub mod ui;

use std::collections::BTreeMap;

use crate::i18n::Locale;
use crate::model::AppState;
use crate::shell::{WriteGate, write_gate};

/// What a screen model reads: the daemon state, the time and the results of the writes.
pub struct ScreenInput<'a> {
    pub state: &'a AppState,
    pub now_us: u64,
    pub locale: Locale,
    /// When the link to the daemon went down or offline, for the "Since" text.
    pub down_since_us: Option<u64>,
    pub feedback: &'a Feedback,
}

impl ScreenInput<'_> {
    pub fn writes(&self) -> WriteGate {
        write_gate(self.state, self.locale)
    }
}

/// The result of one write to the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Pending,
    Applied {
        at_us: u64,
    },
    /// polkit denied the call.
    NotAuthorized,
    /// The daemon answered with an error. `error` is the last element of its name:
    /// `InvalidArgument`, `InUse`, `ReadOnly`.
    Refused {
        error: String,
    },
}

impl WriteOutcome {
    pub fn from_error(error: &zbus::Error) -> Self {
        match error {
            zbus::Error::MethodError(name, message, _) => {
                let last = |text: &str| text.rsplit('.').next().unwrap_or_default().to_owned();
                match last(name.as_str()).as_str() {
                    "AccessDenied" => Self::NotAuthorized,
                    // `org.freedesktop.DBus.Error.Failed` carries the daemon error in its text.
                    "Failed" => Self::Refused {
                        error: message.as_deref().map(last).unwrap_or_default(),
                    },
                    other => Self::Refused {
                        error: other.to_owned(),
                    },
                }
            }
            zbus::Error::FDO(error) => match &**error {
                zbus::fdo::Error::AccessDenied(_) => Self::NotAuthorized,
                // The daemon error is in the text, as for `org.freedesktop.DBus.Error.Failed`.
                zbus::fdo::Error::Failed(message) => Self::Refused {
                    error: message.rsplit('.').next().unwrap_or_default().to_owned(),
                },
                _ => Self::Refused {
                    error: error.to_string(),
                },
            },
            other => Self::Refused {
                error: other.to_string(),
            },
        }
    }

    /// The text next to a control. `refused` is the text of the write that the control makes,
    /// for example "Not applied: the daemon refused the Curve.".
    pub fn text(&self, locale: Locale, refused: &str) -> String {
        match self {
            Self::Pending => locale.tr("feedback.pending"),
            Self::Applied { at_us } => {
                locale.fmt("feedback.applied", &[("time", &format::clock(*at_us))])
            }
            Self::NotAuthorized => locale.tr("feedback.not_authorized"),
            Self::Refused { .. } => refused.to_owned(),
        }
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Self::NotAuthorized | Self::Refused { .. })
    }
}

/// The feedback next to a control that writes: the text, and "Try again" after a failure.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackLine {
    pub text: String,
    pub failure: bool,
    pub retry: Option<String>,
}

impl FeedbackLine {
    pub fn new(
        outcome: &WriteOutcome,
        locale: Locale,
        refused: &str,
        retry: Option<String>,
    ) -> Self {
        Self {
            text: outcome.text(locale, refused),
            failure: outcome.is_failure(),
            retry: retry.filter(|_| outcome.is_failure()),
        }
    }
}

/// The result of the last write of each control, by `Command::feedback_key`.
#[derive(Clone, Debug, Default)]
pub struct Feedback(BTreeMap<String, WriteOutcome>);

impl Feedback {
    pub fn set(&mut self, key: String, outcome: WriteOutcome) {
        self.0.insert(key, outcome);
    }

    pub fn get(&self, key: &str) -> Option<&WriteOutcome> {
        self.0.get(key)
    }
}
