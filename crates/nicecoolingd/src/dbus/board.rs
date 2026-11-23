use std::sync::Arc;

use zbus::interface;

use super::Hub;

pub struct Board {
    hub: Arc<Hub>,
}

impl Board {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.Board")]
impl Board {
    #[zbus(property(emits_changed_signal = "const"))]
    fn vendor(&self) -> String {
        self.hub.board().vendor
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn model(&self) -> String {
        self.hub.board().model
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn board_file(&self) -> String {
        self.hub.board().board_file
    }
}
