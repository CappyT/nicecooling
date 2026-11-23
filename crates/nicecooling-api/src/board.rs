//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Board`.

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Board",
    default_service = "io.github.cappyt.NiceCooling1",
    default_path = "/io/github/cappyt/NiceCooling1/board"
)]
pub trait Board {
    #[zbus(property)]
    fn vendor(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn board_file(&self) -> zbus::Result<String>;
}
