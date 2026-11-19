//! First-person 3D starter template.

use std::path::Path;

use crate::error::CoreError;
use crate::template_view3d;

pub fn write(folder: &Path, _name: &str) -> Result<(), CoreError> {
    template_view3d::write_shared(folder, true)?;
    let scene = template_view3d::build_arena(true);
    std::fs::write(folder.join("scene.seal"), scene.to_text())?;
    Ok(())
}

#[cfg(all(test, feature = "play-preview"))]
mod tests {
    use super::*;
    use seal_world::PlayMode;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-tpl-fps-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn graphs_compile() {
        template_view3d::parse_all().expect("graphs");
    }

    #[test]
    fn play_moves_the_player_with_input() {
        let dir = scratch("move");
        crate::project::create_project(&dir, "FPS").unwrap();
        write(&dir, "FPS").unwrap();
        let mut w = seal_world::World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap())
            .unwrap();
        w.project_dir = Some(dir.clone());
        w.play();
        assert_eq!(w.mode, PlayMode::Playing);
        let x0 = w.entities.iter().find(|e| e.tag == "player").unwrap().translation[0];
        let mut moved = false;
        for _ in 0..60 {
            w.play_keys.push(("W".into(), true));
            w.play_keys.push(("SpaceBar".into(), true));
            assert!(w.step(1.0 / 60.0), "{:?}", w.play_log);
            let p = w.entities.iter().find(|e| e.tag == "player").unwrap();
            if p.translation[2] < 1.5 || p.translation[1] > 1.2 || (p.translation[0] - x0).abs() > 0.2 {
                moved = true;
            }
        }
        assert!(moved, "scripted input moved the player, x0 {x0} log {:?}", w.play_log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_camera_sits_at_the_eyes_and_turns_with_the_mouse() {
        let dir = scratch("eyes");
        crate::project::create_project(&dir, "FPS").unwrap();
        write(&dir, "FPS").unwrap();
        let mut w = seal_world::World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap())
            .unwrap();
        w.project_dir = Some(dir.clone());
        w.play();
        // Mouse look runs while the right mouse button is held.
        let right = seal_world::seal_input::Button::parse("mouse.right").unwrap();
        w.controls.mouse_delta = [20.0, 0.0];
        assert!(w.step(1.0 / 60.0));
        let still = w.entities.iter().find(|e| e.tag == "player").unwrap().rotation[1];
        assert!(still.abs() < 1e-3, "no turning without the button: {still}");
        for _ in 0..5 {
            w.controls.raw.press(right.clone());
            // The window reads the frame's devices into actions before each step.
            let _ = w.controls.frame();
            w.controls.mouse_delta = [20.0, 0.0];
            assert!(w.step(1.0 / 60.0), "{:?}", w.play_log);
        }
        let player = w.entities.iter().find(|e| e.tag == "player").unwrap().clone();
        let cam = w.entities.iter().find(|e| e.tag == "camera").unwrap();
        // Moving the mouse right turns the view right (yaw goes up, see `camera_wish`).
        assert!(player.rotation[1] > 1.0, "player rotation {:?}", player.rotation);
        assert!((cam.rotation[1] - player.rotation[1]).abs() < 5.0, "{:?}", cam.rotation);
        let lift = cam.translation[1] - player.translation[1];
        let flat = (cam.translation[0] - player.translation[0]).hypot(cam.translation[2] - player.translation[2]);
        assert!((lift - 0.6).abs() < 0.2 && flat < 0.3, "camera {:?} player {:?}", cam.translation, player.translation);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mouse_look_tutorial_step_detects_pointer_motion() {
        use crate::project_tutorials::{load_done, tick, write_shipped};
        use crate::{CommandArgs, EditorSession, Layout};

        let dir = scratch("look");
        crate::project::create_project(&dir, "FPS").unwrap();
        write(&dir, "FPS").unwrap();
        write_shipped(&dir, "first_person").unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch(
                "file.open-project",
                &CommandArgs::new(&[("path", dir.to_str().unwrap())]),
            )
            .unwrap();
        session.world.play();
        session.world.controls.mouse_delta = [40.0, 0.0];
        tick(&mut session);
        let done = load_done(&dir).unwrap();
        assert!(done.contains("look_around"), "{done:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}