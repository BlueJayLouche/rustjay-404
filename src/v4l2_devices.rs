//! V4L2 device enumeration (Linux only)

/// Info about a V4L2 capture or loopback device.
#[derive(Debug, Clone)]
pub struct V4l2DeviceInfo {
    /// Device path, e.g. `/dev/video0`
    pub path: String,
    /// Card name reported by the driver
    pub card: String,
    /// Driver name
    pub driver: String,
}

impl V4l2DeviceInfo {
    /// Human-readable label for UI combo boxes.
    pub fn display_name(&self) -> String {
        if self.card.is_empty() {
            self.path.clone()
        } else {
            format!("{} ({})", self.card, self.path)
        }
    }
}

/// Enumerate all `/dev/video*` devices.
///
/// Returns two separate lists: capture devices and output (loopback) devices.
/// Detection is best-effort; unreadable devices are silently skipped.
pub fn enumerate_devices() -> (Vec<V4l2DeviceInfo>, Vec<V4l2DeviceInfo>) {
    let mut capture = Vec::new();
    let mut output = Vec::new();

    for index in 0..32u32 {
        let path = format!("/dev/video{}", index);
        if !std::path::Path::new(&path).exists() {
            continue;
        }

        // Try to read the driver / card name from sysfs.
        let card = read_sysfs_attr(index, "name").unwrap_or_default();
        let driver = read_sysfs_attr(index, "driver").unwrap_or_default();

        let info = V4l2DeviceInfo {
            path: path.clone(),
            card: card.trim().to_string(),
            driver: driver.trim().to_string(),
        };

        // Heuristic: loopback devices are often labelled "v4l2loopback" or
        // have "Dummy" / "loopback" in their card name.
        let is_loopback = info.driver.contains("v4l2loopback")
            || info.card.to_lowercase().contains("loopback")
            || info.card.to_lowercase().contains("dummy");

        if is_loopback {
            output.push(info);
        } else {
            capture.push(info);
        }
    }

    (capture, output)
}

fn read_sysfs_attr(index: u32, attr: &str) -> Option<String> {
    let sysfs_path = format!("/sys/class/video4linux/video{}/{}", index, attr);
    std::fs::read_to_string(&sysfs_path).ok()
}
