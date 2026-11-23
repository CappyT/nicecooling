//! A fake StatusNotifierWatcher and a fake tray host. They follow the interface files that the
//! AppIndicator extension ships: `StatusNotifierWatcher.xml`, `StatusNotifierItem.xml` and
//! `DBusMenu.xml`. The host reads the item and calls its menu like the shell does.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::{Connection, Proxy, interface};
use zbus::{OwnedVariant as OwnedValue, Variant as Value};

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: &str) {
        self.items.lock().expect("items").push(service.to_owned());
    }

    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().expect("items").clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// One entry of the menu layout: id, label, enabled and the children.
#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub kind: String,
    pub children: Vec<MenuEntry>,
}

impl MenuEntry {
    pub fn find(&self, label: &str) -> Option<&MenuEntry> {
        if self.label == label {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(label))
    }

    pub fn labels(&self) -> Vec<String> {
        self.children
            .iter()
            .map(|child| child.label.clone())
            .collect()
    }
}

pub struct FakeTrayHost {
    conn: Connection,
    items: Arc<Mutex<Vec<String>>>,
}

/// `ToolTip`: icon name, icon pixmaps, title and description.
type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

impl FakeTrayHost {
    pub async fn start(conn: Connection) -> Self {
        let items = Arc::new(Mutex::new(Vec::new()));
        conn.object_server()
            .at(
                "/StatusNotifierWatcher",
                Watcher {
                    items: items.clone(),
                },
            )
            .await
            .expect("watcher");
        conn.request_name("org.kde.StatusNotifierWatcher")
            .await
            .expect("watcher name");
        Self { conn, items }
    }

    pub fn registered(&self) -> Vec<String> {
        self.items.lock().expect("items").clone()
    }

    /// The name that the tray registered.
    pub async fn item_name(&self) -> String {
        loop {
            if let Some(name) = self.registered().first() {
                return name.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    async fn item(&self, name: &str) -> Proxy<'_> {
        Proxy::new(
            &self.conn,
            name.to_owned(),
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
        )
        .await
        .expect("item proxy")
    }

    async fn menu(&self, name: &str) -> Proxy<'_> {
        Proxy::new(
            &self.conn,
            name.to_owned(),
            "/MenuBar",
            "com.canonical.dbusmenu",
        )
        .await
        .expect("menu proxy")
    }

    pub async fn string_property(&self, name: &str, property: &str) -> String {
        self.item(name)
            .await
            .get_property(property)
            .await
            .expect("property")
    }

    /// The tooltip title.
    pub async fn tooltip(&self, name: &str) -> String {
        let tooltip: ToolTip = self
            .item(name)
            .await
            .get_property("ToolTip")
            .await
            .expect("ToolTip");
        tooltip.2
    }

    pub async fn layout(&self, name: &str) -> MenuEntry {
        let (_revision, root): (u32, Layout) = self
            .menu(name)
            .await
            .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
            .await
            .expect("GetLayout");
        entry(root)
    }

    /// Click an entry, as the shell does.
    pub async fn click(&self, name: &str, id: i32) {
        self.menu(name)
            .await
            .call::<_, _, ()>("Event", &(id, "clicked", Value::from(0_i32), 0_u32))
            .await
            .expect("Event");
    }

    /// Left click on the icon.
    pub async fn activate(&self, name: &str) {
        self.item(name)
            .await
            .call::<_, _, ()>("Activate", &(0_i32, 0_i32))
            .await
            .expect("Activate");
    }
}

fn entry((id, properties, children): Layout) -> MenuEntry {
    let text = |key: &str| {
        properties
            .get(key)
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
    };
    let enabled = properties
        .get("enabled")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(true);
    MenuEntry {
        id,
        label: text("label").unwrap_or_default(),
        enabled,
        kind: text("type").unwrap_or_else(|| "standard".into()),
        children: children
            .into_iter()
            .map(|child| entry(Layout::try_from(child).expect("child layout")))
            .collect(),
    }
}
