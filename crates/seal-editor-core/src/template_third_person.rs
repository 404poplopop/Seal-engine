//! Third-person 3D starter template.

use std::path::Path;

use crate::error::CoreError;
use crate::template_view3d;

pub fn write(folder: &Path, _name: &str) -> Result<(), CoreError> {
    template_view3d::write_shared(folder, false)?;
    let scene = template_view3d::build_arena(false);
    std::fs::write(folder.join("scene.seal"), scene.to_text())?;
    Ok(())
}

#[cfg(all(test, feature = "play-preview"))]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-tpl-tps-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn player_is_visible_and_moves() {
        let dir = scratch("move");
        crate::project::create_project(&dir, "TPS").unwrap();
        write(&dir, "TPS").unwrap();
        let mut w =
            seal_world::World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap())
                .unwrap();
        w.project_dir = Some(dir.clone());
        assert!(
            w.entities.iter().find(|e| e.tag == "player").is_some_and(|p| !p.hidden),
            "third-person shows the capsule"
        );
        w.play();
        let z0 = w.entities.iter().find(|e| e.tag == "player").unwrap().translation[2];
        let mut moved = false;
        for _ in 0..60 {
            w.play_keys.push(("W".into(), true));
            w.step(1.0 / 60.0);
            let z = w.entities.iter().find(|e| e.tag == "player").unwrap().translation[2];
            if z < z0 - 0.15 {
                moved = true;
            }
        }
        assert!(moved, "WASD moves the player from z {z0}");
        // Looking through the camera, forward walks away from it and the camera keeps up.
        w.set_look_camera(true);
        for step in 0..60 {
            w.play_keys.push(("W".into(), true));
            w.step(1.0 / 60.0);
            let cam = w.entities.iter().find(|e| e.tag == "camera").unwrap();
            let player = w.entities.iter().find(|e| e.tag == "player").unwrap();
            let flat = (cam.translation[0] - player.translation[0])
                .hypot(cam.translation[2] - player.translation[2]);
            assert!(
                flat < 5.6,
                "step {step}: camera {:?} player {:?}",
                cam.translation,
                player.translation
            );
        }
        // The camera sits behind and above the player, turned to look at it.
        let cam = w.entities.iter().find(|e| e.tag == "camera").unwrap();
        let player = w.entities.iter().find(|e| e.tag == "player").unwrap();
        let d = [0, 1, 2].map(|i| cam.translation[i] - player.translation[i]);
        let flat = d[0].hypot(d[2]);
        assert!(
            flat > 1.0 && d[1] > 0.5,
            "camera {:?} {:?} player {:?} log {:?}",
            cam.translation,
            cam.rotation,
            player.translation,
            w.play_log
        );
        let yaw = cam.rotation[1].to_radians();
        let forward = [yaw.sin(), yaw.cos()];
        let dot = (forward[0] * -d[0] + forward[1] * -d[2]) / flat;
        assert!(dot > 0.95, "camera rotation {:?} faces the player ({dot})", cam.rotation);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
