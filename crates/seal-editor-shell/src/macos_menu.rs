//! Global menu bar on macOS, wired to the same commands as the in-window menus.

use std::collections::HashMap;

use muda::{Menu, MenuEvent, MenuId, MenuItem, Submenu};
use seal_editor_core::{menu_mnemonic_parts, EditorSession, PanelRegistry, MENU_CATEGORIES};

pub struct MacMenu {
    commands: HashMap<MenuId, String>,
}

impl MacMenu {
    pub fn install(session: &EditorSession, registry: &PanelRegistry) -> Self {
        let bar = Menu::new();
        let mut commands = HashMap::new();
        for category in MENU_CATEGORIES {
            let ids: Vec<String> = session
                .menu_ids(category)
                .into_iter()
                .filter(|id| session.commands.get(id).is_some_and(|spec| spec.in_menu))
                .collect();
            let panels = *category == "window";
            if ids.is_empty() && !panels {
                continue;
            }
            let raw = session.messages.get(&format!("menu-{category}"));
            let (title, _) = menu_mnemonic_parts(&raw);
            let submenu = Submenu::new(&title, true);
            for id in ids {
                let label = session
                    .commands
                    .get(&id)
                    .map(|spec| session.messages.get(&spec.title_key))
                    .unwrap_or_else(|| id.clone());
                let (visible, _) = menu_mnemonic_parts(&label);
                let item = MenuItem::new(visible, true, None);
                commands.insert(item.id().clone(), id);
                let _ = submenu.append(&item);
            }
            if panels {
                let _ = submenu.append(&muda::PredefinedMenuItem::separator());
                for panel in registry.iter() {
                    let title = session.messages.get(&panel.title_key);
                    let item = MenuItem::new(title, true, None);
                    commands.insert(item.id().clone(), format!("dock:{}", panel.id));
                    let _ = submenu.append(&item);
                }
            }
            let _ = bar.append(&submenu);
        }
        let _ = bar.init_for_nsapp();
        Self { commands }
    }

    pub fn poll(&self, run: &mut dyn FnMut(&str)) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(command) = self.commands.get(event.id()) {
                run(command);
            }
        }
    }
}
