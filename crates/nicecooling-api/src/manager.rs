//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Manager`.

use zbus::OwnedObjectPath;

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Manager",
    default_service = "io.github.cappyt.NiceCooling1",
    default_path = "/io/github/cappyt/NiceCooling1"
)]
pub trait Manager {
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn state_health(&self) -> zbus::Result<String>;

    fn create_derived_sensor(
        &self,
        name: &str,
        r#type: &str,
        sources: &[OwnedObjectPath],
    ) -> zbus::Result<OwnedObjectPath>;
    fn create_loop(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    /// (id, name, preset, points as (raw rpm, flow L/h))
    fn get_calibration_tables(&self) -> zbus::Result<Vec<(String, String, bool, Vec<(f64, f64)>)>>;
    fn set_calibration_table(
        &self,
        id: &str,
        name: &str,
        points: &[(f64, f64)],
    ) -> zbus::Result<String>;
    fn delete_calibration_table(&self, id: &str) -> zbus::Result<()>;

    /// `sensors` holds (id, value, flags). `channels` holds (id, Duty, smoothed Input, Curve
    /// target, flags).
    #[zbus(signal)]
    fn sensors_updated(
        &self,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, f64, f64, u8)>,
    ) -> zbus::Result<()>;
}
