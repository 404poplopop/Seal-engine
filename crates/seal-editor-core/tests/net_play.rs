use seal_editor_core::{CommandArgs, EditorSession, Layout, PlayMode};

#[test]
fn one_window_hosts_and_another_joins_from_the_tools_commands() {
    let mut host = EditorSession::new(Layout::default());
    host.dispatch("play.host", &CommandArgs::new(&[("port", "0"), ("interface", "127.0.0.1")]))
        .unwrap();
    assert_eq!(host.world.mode, PlayMode::Playing);
    let Some(seal_world::net::NetRole::Host(h)) = host.world.net.as_ref() else {
        panic!("hosting")
    };
    let port = h.port().to_string();
    assert!(host.notice.as_deref().unwrap_or("").contains(&port));

    let mut guest = EditorSession::new(Layout::default());
    guest
        .dispatch("play.join", &CommandArgs::new(&[("address", &format!("127.0.0.1:{port}"))]))
        .unwrap();
    assert_eq!(guest.world.mode, PlayMode::Playing);

    let id = host.world.entities[1].id.clone();
    for frame in 0..60 {
        guest.world.step(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(2));
        host.world.entities[1].translation[1] = 1.0 + frame as f32 * 0.05;
        host.world.step(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let shown = guest.world.entities.iter().find(|e| e.id == id).unwrap().translation[1];
    assert!(shown > 3.0, "the joined window shows the host's game: {shown}");

    guest.dispatch("play.stop", &CommandArgs::default()).unwrap();
    assert!(guest.world.net.is_none());
    host.dispatch("play.stop", &CommandArgs::default()).unwrap();
    assert!(host.world.net.is_none());
}

#[test]
fn a_bad_port_is_refused_without_starting_play() {
    let mut session = EditorSession::new(Layout::default());
    assert!(session.dispatch("play.host", &CommandArgs::new(&[("port", "nope")])).is_err());
    assert_eq!(session.world.mode, PlayMode::Editing);
}
