//! Output preview window - shows mixer output inside the control window

use imgui::Ui;

/// Preview window displaying the mixer output
pub struct PreviewWindow {
    /// ImGui texture ID for the preview
    pub texture_id: Option<imgui::TextureId>,
    /// Preview texture dimensions (matches output)
    pub width: u32,
    pub height: u32,
}

impl PreviewWindow {
    pub fn new() -> Self {
        Self {
            texture_id: None,
            width: 0,
            height: 0,
        }
    }

    /// Draw the preview window
    pub fn draw(&self, ui: &Ui) {
        let tex_id = match self.texture_id {
            Some(id) => id,
            None => return,
        };

        if self.width == 0 || self.height == 0 {
            return;
        }

        ui.window("Preview")
            .size([420.0, 260.0], imgui::Condition::FirstUseEver)
            .build(|| {
                let avail = ui.content_region_avail();
                if avail[0] <= 0.0 || avail[1] <= 0.0 {
                    return;
                }

                // Calculate display size maintaining aspect ratio
                let content_aspect = self.width as f32 / self.height as f32;
                let container_aspect = avail[0] / avail[1];

                let (display_w, display_h) = if content_aspect > container_aspect {
                    // Content wider than container - fit to width
                    (avail[0], avail[0] / content_aspect)
                } else {
                    // Content taller than container - fit to height
                    (avail[1] * content_aspect, avail[1])
                };

                // Center the image
                let pad_x = (avail[0] - display_w) * 0.5;
                if pad_x > 0.0 {
                    ui.set_cursor_pos([ui.cursor_pos()[0] + pad_x, ui.cursor_pos()[1]]);
                }

                imgui::Image::new(tex_id, [display_w, display_h]).build(ui);
            });
    }
}

impl Default for PreviewWindow {
    fn default() -> Self {
        Self::new()
    }
}
