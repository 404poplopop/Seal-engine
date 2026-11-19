//! Seal Hop: run, jump, collect every coin, reach the flag.
//!
//! The level is a text grid (`#` ground, `o` coin, `P` start, `F` flag),
//! one character per world unit. Everything uses the engine's own
//! pieces: sprites, the 2D camera, 2D physics with sensors and rays,
//! input actions and axes, and mixed sound.

use seal_app::audio::Sound;
use seal_app::ecs::core::{Transform, SceneSettings};
use seal_app::ecs::Entity;
use seal_app::physics2d::{Body2D, Collider2D, Velocity2D};
use seal_app::render2d::{Camera2D, Sprite};
use seal_app::{AppConfig, Ctx, Game, SAMPLE_RATE};

pub const DEFAULT_INPUT: &str = include_str!("../data/input.map");
pub const DEFAULT_LEVEL: &str = include_str!("../data/level.txt");

pub fn config() -> AppConfig {
    AppConfig { title: "Seal Hop".into(), width: 640, height: 360, input: DEFAULT_INPUT.into() }
}

const RUN_SPEED: f32 = 6.0;
const JUMP_SPEED: f32 = 12.0;
const PLAYER_SIZE: [f32; 2] = [0.8, 0.9];
const FALL_LIMIT: f32 = -4.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Playing,
    Won { at_frame: u64 },
}

pub struct SealHop {
    level: String,
    pub player: Entity,
    pub start: [f32; 2],
    pub coins_total: usize,
    /// Coin entities, left to right as built.
    pub coin_list: Vec<Entity>,
    pub coins: usize,
    pub falls: u32,
    pub jumps: u32,
    pub state: State,
    facing_left: bool,
    flag: Entity,
    hud: Vec<Entity>,
    level_width: f32,
    /// When set, the bot plays instead of the player.
    pub demo: Option<Autopilot>,
}

impl SealHop {
    pub fn new(level: &str) -> Self {
        Self {
            level: level.to_string(),
            player: Entity::from_bits(0),
            start: [0.0, 0.0],
            coins_total: 0,
            coin_list: Vec::new(),
            coins: 0,
            falls: 0,
            jumps: 0,
            state: State::Playing,
            facing_left: false,
            flag: Entity::from_bits(0),
            hud: Vec::new(),
            level_width: 0.0,
            demo: None,
        }
    }

    fn at(x: f32, y: f32) -> Transform {
        Transform { translation: [x, y, 0.0], ..Default::default() }
    }

    fn build(&mut self, cx: &mut Ctx) {
        cx.world = {
            let mut w = seal_app::ecs::World::new();
            w.register_core();
            seal_app::render2d::register_2d(&mut w);
            seal_app::physics2d::register_physics2d(&mut w);
            w
        };
        cx.physics = seal_app::physics2d::Physics2D::new();
        cx.world.insert_resource(SceneSettings {
            gravity: 24.0,
            time_scale: 1.0,
            music: String::new(),
        });
        cx.world.insert_resource(Camera2D {
            center: [10.0, 5.625],
            zoom: 32.0,
            clear: [0.53, 0.78, 0.93, 1.0],
        });
        let rows: Vec<&str> = self.level.lines().collect();
        let height = rows.len();
        self.level_width = rows.iter().map(|r| r.len()).max().unwrap_or(0) as f32;
        self.coins_total = 0;
        self.coin_list.clear();
        self.coins = 0;
        self.state = State::Playing;
        for (row, line) in rows.iter().enumerate() {
            // Row 0 is the top; world y grows upward, one unit per row, centred on cells.
            let y = (height - 1 - row) as f32 + 0.5;
            let bytes = line.as_bytes();
            let mut x = 0;
            while x < bytes.len() {
                let cx_ = x as f32 + 0.5;
                match bytes[x] {
                    b'#' => {
                        let start = x;
                        while x < bytes.len() && bytes[x] == b'#' {
                            let top = row == 0 || rows[row - 1].as_bytes().get(x) != Some(&b'#');
                            cx.world.spawn((
                                Self::at(x as f32 + 0.5, y),
                                Sprite {
                                    image: if top { "grass" } else { "dirt" }.into(),
                                    ..Default::default()
                                },
                            ));
                            x += 1;
                        }
                        let len = (x - start) as f32;
                        cx.world.spawn((
                            Self::at(start as f32 + len * 0.5, y),
                            Body2D::fixed(),
                            Collider2D { size: [len, 1.0], friction: 0.0, ..Default::default() },
                        ));
                        continue;
                    }
                    b'o' => {
                        self.coins_total += 1;
                        let coin = cx.world.spawn((
                            Self::at(cx_, y),
                            Sprite {
                                image: "coin".into(),
                                size: [0.6, 0.6],
                                layer: 1,
                                ..Default::default()
                            },
                            Body2D::fixed(),
                            Collider2D {
                                shape: "circle".into(),
                                size: [0.6, 0.6],
                                sensor: true,
                                ..Default::default()
                            },
                        ));
                        self.coin_list.push(coin);
                    }
                    b'F' => {
                        self.flag = cx.world.spawn((
                            Self::at(cx_, y + 0.5),
                            Sprite {
                                image: "flag".into(),
                                size: [1.0, 2.0],
                                layer: 1,
                                ..Default::default()
                            },
                            Body2D::fixed(),
                            Collider2D { size: [0.6, 2.0], sensor: true, ..Default::default() },
                        ));
                    }
                    b'P' => self.start = [cx_, y],
                    _ => {}
                }
                x += 1;
            }
        }
        self.player = cx.world.spawn((
            Self::at(self.start[0], self.start[1]),
            Sprite { image: "seal".into(), size: [1.0, 1.0], layer: 2, ..Default::default() },
            Body2D { lock_rotation: true, ..Default::default() },
            Collider2D { size: PLAYER_SIZE, friction: 0.0, ..Default::default() },
            Velocity2D::default(),
        ));
        self.hud = (0..self.coins_total)
            .map(|_| {
                cx.world.spawn((
                    Self::at(0.0, 0.0),
                    Sprite {
                        size: [0.35, 0.35],
                        layer: 10,
                        color: [0.0, 0.0, 0.0, 0.35],
                        ..Default::default()
                    },
                ))
            })
            .collect();
    }

    pub fn grounded(&self, cx: &Ctx) -> bool {
        let Some(t) = cx.world.get::<Transform>(self.player) else { return false };
        let [x, y, _] = t.translation;
        let foot = y - PLAYER_SIZE[1] * 0.5 - 0.01;
        [-0.35, 0.0, 0.35].iter().any(|dx| {
            cx.physics.raycast([x + dx, foot], [0.0, -1.0], 0.08).is_some_and(|hit| {
                cx.world.get::<Collider2D>(hit.entity).is_some_and(|c| !c.sensor)
            })
        })
    }

    fn pan_of(&self, cx: &Ctx, x: f32) -> f32 {
        let center = cx.world.resource::<Camera2D>().map(|c| c.center[0]).unwrap_or(x);
        (x - center) / 10.0
    }

    fn respawn(&mut self, cx: &mut Ctx) {
        self.falls += 1;
        if let Some(t) = cx.world.get_mut::<Transform>(self.player) {
            t.translation = [self.start[0], self.start[1], 0.0];
        }
        if let Some(v) = cx.world.get_mut::<Velocity2D>(self.player) {
            *v = Velocity2D::default();
        }
        cx.play("fall", 0.5, 0.0);
    }

    fn place_camera_and_hud(&mut self, cx: &mut Ctx) {
        let x = cx.world.get::<Transform>(self.player).map(|t| t.translation[0]).unwrap_or(0.0);
        let half = 10.0;
        let center_x = x.clamp(half, (self.level_width - half).max(half));
        if let Some(cam) = cx.world.resource_mut::<Camera2D>() {
            cam.center[0] = center_x;
        }
        for (i, e) in self.hud.clone().into_iter().enumerate() {
            let collected = i < self.coins;
            if let Some(t) = cx.world.get_mut::<Transform>(e) {
                t.translation = [center_x - 9.4 + i as f32 * 0.45, 10.6, 0.0];
            }
            if let Some(s) = cx.world.get_mut::<Sprite>(e) {
                s.color = if collected { [1.0, 0.85, 0.2, 1.0] } else { [0.0, 0.0, 0.0, 0.35] };
            }
        }
        if let State::Won { at_frame } = self.state {
            // Celebrate: the flag pulses.
            let t = (cx.frame - at_frame) as f32 * 0.2;
            if let Some(s) = cx.world.get_mut::<Sprite>(self.flag) {
                let glow = 0.75
                    + 0.25
                        * seal_app::render2d::Affine2::from_transform(&Transform {
                            rotation: [0.0, 0.0, t.to_degrees()],
                            ..Default::default()
                        })
                        .apply([1.0, 0.0])[0];
                s.color = [glow, 1.0, glow, 1.0];
            }
        }
    }
}

/// Small procedural pixel art, so the sample needs no image files.
fn art(cx: &mut Ctx) {
    for (name, w, h, px) in art_images() {
        cx.load_image(name, w, h, px);
    }
}

/// The game's pixel art as (name, width, height, RGBA8 pixels).
pub fn art_images() -> Vec<(&'static str, u32, u32, Vec<u8>)> {
    let mut out = Vec::new();
    let image = |w: u32, h: u32, f: &dyn Fn(u32, u32) -> [u8; 4]| {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        px
    };
    out.push((
        "seal",
        16,
        16,
        image(16, 16, &|x, y| {
            let (dx, dy) = (x as i32 - 8, y as i32 - 9);
            let body = dx * dx * 3 + dy * dy * 5 <= 220;
            let eye = (x == 11 || x == 12) && (y == 6 || y == 7);
            let nose = x == 14 && y == 8;
            let flipper = (x == 3 || x == 4) && y >= 12;
            if eye || nose {
                [20, 24, 40, 255]
            } else if body || flipper {
                let shade = if y > 11 { 150 } else { 185 };
                [shade - 30, shade - 10, shade + 20, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "grass",
        16,
        16,
        image(16, 16, &|x, y| {
            if y < 4 || (y == 4 && x % 3 == 0) {
                [70, 170 + (x * 7 % 20) as u8, 60, 255]
            } else {
                let speck = (x * 5 + y * 3) % 11 == 0;
                if speck {
                    [110, 80, 50, 255]
                } else {
                    [140, 100, 64, 255]
                }
            }
        }),
    ));
    out.push((
        "dirt",
        16,
        16,
        image(16, 16, &|x, y| {
            if (x * 5 + y * 3) % 11 == 0 {
                [110, 80, 50, 255]
            } else {
                [140, 100, 64, 255]
            }
        }),
    ));
    out.push((
        "coin",
        8,
        8,
        image(8, 8, &|x, y| {
            let (dx, dy) = (x as i32 * 2 - 7, y as i32 * 2 - 7);
            let d = dx * dx + dy * dy;
            if d <= 20 && (3..=4).contains(&x) && (2..=5).contains(&y) {
                [255, 245, 170, 255]
            } else if d <= 49 {
                [245, 190, 40, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "flag",
        16,
        32,
        image(16, 32, &|x, y| {
            if x == 2 || x == 3 {
                [90, 90, 100, 255]
            } else if x > 3 && y < 12 && x < 4 + (12 - y.abs_diff(6)) {
                [230, 60, 60, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out
}

fn sounds(cx: &mut Ctx) {
    let r = SAMPLE_RATE;
    let mut coin = Sound::tone(r, 1318.5, 0.07, 0.35);
    coin.samples.extend(Sound::tone(r, 1760.0, 0.09, 0.3).samples);
    cx.add_sound("coin", coin);
    cx.add_sound("jump", Sound::tone(r, 523.25, 0.05, 0.2));
    cx.add_sound("fall", Sound::tone(r, 146.8, 0.25, 0.4));
    let mut win = Sound::tone(r, 523.25, 0.12, 0.3);
    win.samples.extend(Sound::tone(r, 659.25, 0.12, 0.3).samples);
    win.samples.extend(Sound::tone(r, 783.99, 0.3, 0.3).samples);
    cx.add_sound("win", win);
}

impl Game for SealHop {
    fn start(&mut self, cx: &mut Ctx) {
        art(cx);
        sounds(cx);
        self.build(cx);
        self.place_camera_and_hud(cx);
    }

    fn update(&mut self, cx: &mut Ctx) {
        if let Some(mut bot) = self.demo.take() {
            let raw = bot.input(self, cx);
            cx.actions.update(&cx.map, &raw);
            self.demo = Some(bot);
        }
        if cx.actions.just_pressed("quit") {
            cx.quit = true;
        }
        if cx.actions.just_pressed("restart") {
            self.build(cx);
            self.place_camera_and_hud(cx);
            return;
        }
        let grounded = self.grounded(cx);
        let mut jumped = false;
        let axis = cx.actions.axis("move");
        if let Some(v) = cx.world.get_mut::<Velocity2D>(self.player) {
            v.linear[0] = axis * RUN_SPEED;
            if cx.actions.just_pressed("jump") && grounded {
                v.linear[1] = JUMP_SPEED;
                jumped = true;
            }
            // Let go early for a short hop.
            if cx.actions.just_released("jump") && v.linear[1] > 0.0 {
                v.linear[1] *= 0.5;
            }
        }
        if jumped {
            self.jumps += 1;
            cx.play("jump", 0.4, 0.0);
        }
        if axis != 0.0 {
            self.facing_left = axis < 0.0;
        }
        let facing_left = self.facing_left;
        if let Some(s) = cx.world.get_mut::<Sprite>(self.player) {
            s.flip[0] = facing_left;
        }
    }

    fn after_physics(&mut self, cx: &mut Ctx) {
        for contact in cx.physics.contacts.clone() {
            if !contact.started || !contact.sensor {
                continue;
            }
            let other = if contact.a == self.player {
                contact.b
            } else if contact.b == self.player {
                contact.a
            } else {
                continue;
            };
            if other == self.flag {
                if self.state == State::Playing && self.coins == self.coins_total {
                    self.state = State::Won { at_frame: cx.frame };
                    cx.play("win", 0.6, 0.0);
                }
            } else if cx.world.contains(other) {
                let x = cx.world.get::<Transform>(other).map(|t| t.translation[0]).unwrap_or(0.0);
                cx.world.despawn(other);
                self.coins += 1;
                let pan = self.pan_of(cx, x);
                cx.play("coin", 0.5, pan);
            }
        }
        let y = cx.world.get::<Transform>(self.player).map(|t| t.translation[1]).unwrap_or(0.0);
        if y < FALL_LIMIT {
            self.respawn(cx);
        }
        self.place_camera_and_hud(cx);
    }
}

/// A simple bot that plays the level: it heads for the next coin (left to
/// right), then the flag, and jumps at walls, gaps and targets above it.
/// Used for the demo mode and for tests that play the game end to end.
#[derive(Default)]
pub struct Autopilot {
    hold: u32,
}

impl Autopilot {
    pub fn input(&mut self, game: &SealHop, cx: &Ctx) -> seal_app::input::RawInput {
        use seal_app::input::{Button, RawInput};
        let mut raw = RawInput::default();
        if game.state != State::Playing {
            return raw;
        }
        let Some(p) = cx.world.get::<Transform>(game.player).map(|t| t.translation) else {
            return raw;
        };
        let mut targets: Vec<[f32; 3]> = game
            .coin_list
            .iter()
            .filter_map(|e| cx.world.get::<Transform>(*e))
            .map(|t| t.translation)
            .collect();
        targets.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let target = match targets.first() {
            Some(t) => [t[0], t[1]],
            None => cx
                .world
                .get::<Transform>(game.flag)
                .map(|t| [t.translation[0], t.translation[1]])
                .unwrap_or(p[..2].try_into().unwrap()),
        };
        let dx = target[0] - p[0];
        let dir = if dx > 0.2 {
            1.0
        } else if dx < -0.2 {
            -1.0
        } else {
            0.0
        };
        if dir > 0.0 {
            raw.press(Button::key("d"));
        } else if dir < 0.0 {
            raw.press(Button::key("a"));
        }
        if self.hold > 0 {
            self.hold -= 1;
            raw.press(Button::key("space"));
            return raw;
        }
        if dir == 0.0 && target[1] <= p[1] + 0.6 || !game.grounded(cx) {
            return raw;
        }
        let solid = |hit: Option<seal_app::physics2d::RayHit2D>| {
            hit.is_some_and(|h| cx.world.get::<Collider2D>(h.entity).is_some_and(|c| !c.sensor))
        };
        let wall_ahead = solid(cx.physics.raycast([p[0] + dir * 0.41, p[1]], [dir, 0.0], 0.5));
        let ground_ahead = solid(cx.physics.raycast([p[0] + dir * 0.9, p[1]], [0.0, -1.0], 1.5));
        let target_above = target[1] > p[1] + 0.6 && dx.abs() < 3.0;
        if wall_ahead || !ground_ahead || target_above {
            self.hold = 14;
            raw.press(Button::key("space"));
        }
        raw
    }
}
