//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Channel`.

use zbus::OwnedObjectPath;

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Channel",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait Channel {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn default_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn duty(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn raw(&self) -> zbus::Result<u8>;
    /// hardware | curve | fixed
    #[zbus(property)]
    fn control(&self) -> zbus::Result<String>;
    /// none | lost-input | pump-stop
    #[zbus(property)]
    fn r#override(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn is_pump(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn manual_at_snapshot(&self) -> zbus::Result<bool>;
    /// (input sensor, points as (input, duty), smoothing s, step limit %/s)
    #[zbus(property)]
    fn curve(&self) -> zbus::Result<(OwnedObjectPath, Vec<(f64, f64)>, f64, f64)>;
    #[zbus(property)]
    fn fixed_duty(&self) -> zbus::Result<f64>;
    /// (snapshot time in microseconds, attributes as (name, value) in restore order)
    #[zbus(property)]
    fn bios_snapshot(&self) -> zbus::Result<(u64, Vec<(String, String)>)>;
    /// The sensor of the fan speed, `/` when it does not exist.
    #[zbus(property)]
    fn fan_sensor(&self) -> zbus::Result<OwnedObjectPath>;

    fn set_name(&self, name: &str) -> zbus::Result<()>;
    fn set_curve(
        &self,
        input: &OwnedObjectPath,
        points: &[(f64, f64)],
        smoothing_s: f64,
        step_limit_pct_per_s: f64,
    ) -> zbus::Result<()>;
    fn set_fixed_duty(&self, duty: f64) -> zbus::Result<()>;
    fn release_to_hardware(&self) -> zbus::Result<()>;
}
