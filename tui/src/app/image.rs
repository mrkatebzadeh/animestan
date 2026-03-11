use std::collections::{HashMap, HashSet};

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use throbber_widgets_tui::ThrobberState;

#[derive(Default)]
pub struct ImageState {
    images: HashMap<String, Protocol>,
    area: Rect,
    throbber: ThrobberState,
}

impl ImageState {
    pub fn insert_manga(&mut self, id: String, protocol: Protocol) {
        self.images.insert(id, protocol);
    }

    pub fn get_image_state(&self, id: &str) -> Option<&Protocol> {
        self.images.get(id)
    }

    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn throbber_mut(&mut self) -> &mut ThrobberState {
        &mut self.throbber
    }
}

#[derive(Default)]
pub struct ImageUiState {
    pub picker: Option<Picker>,
    pub state: ImageState,
    pub pending: HashSet<String>,
}

impl crate::app::App {
    pub fn set_image_picker(&mut self, picker: Option<Picker>) {
        self.image.picker = picker;
    }

    pub fn can_display_images(&self) -> bool {
        self.image.picker.is_some()
    }

    pub fn image_picker_mut(&mut self) -> Option<&mut Picker> {
        self.image.picker.as_mut()
    }

    pub fn image_state(&self) -> &ImageState {
        &self.image.state
    }

    pub fn image_state_mut(&mut self) -> &mut ImageState {
        &mut self.image.state
    }

    pub fn image_pending(&self, id: &str) -> bool {
        self.image.pending.contains(id)
    }

    pub fn mark_image_pending(&mut self, id: String) {
        self.image.pending.insert(id);
    }

    pub fn clear_image_pending(&mut self, id: &str) {
        self.image.pending.remove(id);
    }
}
