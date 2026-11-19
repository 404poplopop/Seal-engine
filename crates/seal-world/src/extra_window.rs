//! Extra game windows opened while play runs.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtraWindow {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub camera: String,
    pub ui_screen: String,
}

impl ExtraWindow {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.is_empty() {
            return Err("the window title is empty".into());
        }
        if !(64..=8192).contains(&self.width) || !(64..=8192).contains(&self.height) {
            return Err("window width and height are whole numbers from 64 to 8192".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtraWindowTable {
    pub next_id: u32,
    pub open: BTreeMap<u32, ExtraWindow>,
}

impl ExtraWindowTable {
    /// `id` 0 assigns the next id, starting at 1. An explicit id is kept and
    /// the counter moves past it so a later automatic id does not collide.
    pub fn open_window(&mut self, id: u32, spec: ExtraWindow) -> Result<u32, String> {
        spec.validate()?;
        let window_id = if id == 0 {
            let out = self.next_id.max(1);
            self.next_id = out.saturating_add(1);
            out
        } else {
            self.next_id = self.next_id.max(id.saturating_add(1));
            id
        };
        self.open.insert(window_id, spec);
        Ok(window_id)
    }

    pub fn move_window(&mut self, id: u32, x: i32, y: i32) -> Result<(), String> {
        let entry = self.open.get_mut(&id).ok_or_else(|| format!("window {id} is not open"))?;
        entry.x = x;
        entry.y = y;
        Ok(())
    }

    pub fn resize_window(&mut self, id: u32, width: u32, height: u32) -> Result<(), String> {
        if !(64..=8192).contains(&width) || !(64..=8192).contains(&height) {
            return Err("window width and height are whole numbers from 64 to 8192".into());
        }
        let entry = self.open.get_mut(&id).ok_or_else(|| format!("window {id} is not open"))?;
        entry.width = width;
        entry.height = height;
        Ok(())
    }

    pub fn close_window(&mut self, id: u32) -> Result<(), String> {
        self.open.remove(&id).ok_or_else(|| format!("window {id} is not open"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_move_resize_and_close() {
        let mut table = ExtraWindowTable::default();
        let spec = ExtraWindow {
            title: "Test".into(),
            width: 640,
            height: 480,
            x: 0,
            y: 0,
            camera: String::new(),
            ui_screen: String::new(),
        };
        let id = table.open_window(0, spec).unwrap();
        assert_eq!(id, 1);
        table.move_window(id, 10, 20).unwrap();
        table.resize_window(id, 800, 600).unwrap();
        assert_eq!(table.open.get(&id).unwrap().width, 800);
        table.close_window(id).unwrap();
        assert!(table.open.is_empty());
    }
}
