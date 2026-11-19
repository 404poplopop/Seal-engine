use seal_world::nav::NavGrid;

fn grid(rows: &[&str]) -> NavGrid {
    let mut g = NavGrid::new(rows[0].len(), rows.len(), 1.0, (0.0, 0.0));
    for (y, r) in rows.iter().enumerate() {
        for (x, c) in r.chars().enumerate() {
            g.set_blocked(x, y, c == '#');
        }
    }
    g
}

#[test]
fn straight_and_diagonal_paths_are_shortest() {
    let g = grid(&["....", "....", "....", "...."]);
    let p = g.find_path((0.5, 0.5), (3.5, 0.5)).unwrap();
    assert_eq!(p.len(), 4);
    let d = g.find_path((0.5, 0.5), (3.5, 3.5)).unwrap();
    assert_eq!(d.len(), 4);
    assert_eq!(d[3], (3.5, 3.5));
}

#[test]
fn walls_are_routed_around_and_corners_not_cut() {
    let g = grid(&["..#..", "..#..", "..#..", "....."]);
    let p = g.find_path((0.5, 0.5), (4.5, 0.5)).unwrap();
    assert!(p
        .iter()
        .all(|c| g.cell_of(*c).map(|(x, y)| g.is_walkable(x as i64, y as i64)) == Some(true)));
    assert!(p.iter().any(|c| c.1 > 3.0));
    let g2 = grid(&[".#", "#."]);
    assert!(
        g2.find_path((0.5, 0.5), (1.5, 1.5)).is_none(),
        "no diagonal squeeze between two walls"
    );
}

#[test]
fn unreachable_blocked_or_outside_give_none_and_result_is_deterministic() {
    let g = grid(&[".#.", ".#.", ".#."]);
    assert!(g.find_path((0.5, 0.5), (2.5, 0.5)).is_none());
    assert!(g.find_path((1.5, 0.5), (0.5, 0.5)).is_none());
    assert!(g.find_path((0.5, 0.5), (9.0, 9.0)).is_none());
    let o = grid(&["....", ".##.", "...."]);
    assert_eq!(o.find_path((0.5, 0.5), (3.5, 2.5)), o.find_path((0.5, 0.5), (3.5, 2.5)));
}
