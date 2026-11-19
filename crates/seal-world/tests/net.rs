use seal_world::net::{Client, Snapshot};
use seal_world::World;

#[test]
fn snapshots_round_trip_and_reject_garbage() {
    let w = World::demo();
    let s = Snapshot::capture(&w, 7, 1.5);
    assert_eq!(Snapshot::decode(&s.encode()).unwrap(), s);
    assert!(Snapshot::decode(b"nope").is_err());
    assert!(Snapshot::decode(&s.encode()[..10]).is_err());
}

#[test]
fn a_client_world_follows_the_server_with_interpolation_and_drops_stale_packets() {
    let mut server = World::demo();
    let mut client_world = World::demo();
    let mut client = Client::default();
    let id = server.entities[1].id.clone();
    for (seq, x) in [(1u32, 0.0f32), (2, 10.0)] {
        server.entities[1].translation[0] = x;
        assert!(client.receive(&Snapshot::capture(&server, seq, seq as f32 * 0.1).encode()));
    }
    assert!(!client.receive(&Snapshot::capture(&server, 1, 0.1).encode()), "stale");
    client.apply(&mut client_world, 0.15);
    let e = client_world.entities.iter().find(|e| e.id == id).unwrap();
    assert!((e.translation[0] - 5.0).abs() < 1e-4);
    client.apply(&mut client_world, 9.0);
    assert_eq!(client_world.entities.iter().find(|e| e.id == id).unwrap().translation[0], 10.0);
}

#[test]
fn snapshots_cross_a_real_udp_socket_on_localhost() {
    use std::net::UdpSocket;
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
    let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut server = World::demo();
    server.entities[0].translation = [1.0, 2.0, 3.0];
    tx.send_to(&Snapshot::capture(&server, 1, 0.0).encode(), rx.local_addr().unwrap()).unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    let mut client = Client::default();
    assert!(client.receive(&buf[..n]));
    let mut cw = World::demo();
    client.apply(&mut cw, 0.0);
    assert_eq!(cw.entities[0].translation, [1.0, 2.0, 3.0]);
}

#[test]
fn an_input_packet_round_trips() {
    use seal_world::net::InputMsg;
    let msg = InputMsg {
        seq: 7,
        input: seal_world::PlayInput { left: false, right: true, jump: true },
        keys: vec![("space".into(), true)],
    };
    assert_eq!(InputMsg::decode(&msg.encode()).unwrap(), msg);
    assert!(InputMsg::decode(b"SNP1....").is_err());
}

#[test]
fn a_joined_world_shows_the_host_game_and_drives_it_with_its_input() {
    use seal_world::net::{Host, Join, NetRole};
    let mut host = World::demo();
    let host_net = Host::bind_on("127.0.0.1", 0).unwrap();
    let port = host_net.port();
    host.net = Some(NetRole::Host(host_net));
    host.play();

    let mut guest = World::demo();
    guest.net = Some(NetRole::Join(Join::connect(&format!("127.0.0.1:{port}")).unwrap()));
    guest.play();
    guest.play_input.right = true;
    guest.play_keys.push(("e".into(), true));

    let id = host.entities[1].id.clone();
    let mut host_keys = Vec::new();
    for frame in 0..120 {
        guest.step(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(2));
        host.entities[1].translation[0] = frame as f32 * 0.1;
        let before = host.play_keys.len();
        host.step(1.0 / 60.0);
        let _ = before;
        host_keys.extend(host.play_keys.drain(..));
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(host.play_input.right, "the guest's held button reaches the host");
    assert_eq!(
        host_keys,
        vec![("e".to_string(), true)],
        "the guest's key press reaches the host once"
    );
    let Some(NetRole::Host(h)) = host.net.as_ref() else { panic!() };
    assert_eq!(h.peer_count(), 1);
    let Some(NetRole::Join(j)) = guest.net.as_ref() else { panic!() };
    assert!(j.connected());
    let shown = guest.entities.iter().find(|e| e.id == id).unwrap().translation[0];
    assert!(
        shown > 8.0 && shown <= 11.9,
        "the guest shows the host's object slightly behind: {shown}"
    );
    guest.stop();
    assert!(guest.net.is_none(), "stopping play leaves network play");
}
