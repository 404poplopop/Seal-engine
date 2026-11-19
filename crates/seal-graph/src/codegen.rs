//! Turns a [`Program`] into Rust source.
//!
//! The output runs the same steps as the interpreter in the same order and
//! calls the same [`crate::rt`] functions, so a compiled graph and the
//! interpreter give bit-identical results. Data expressions are inlined as
//! typed Rust; execution nodes become arms of one `match`.

use crate::rt::{Ty, Value};
use crate::ListOp;
use crate::{BinOp, CmpOp, Op, Program, Src, UnOp};
use std::fmt::Write;

/// A Rust module name for a graph name.
pub fn module_name(graph: &str) -> String {
    let mut s: String = graph
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "g_");
    }
    s
}

fn lit(v: &Value) -> String {
    let f = |x: f64| format!("f64::from_bits(0x{:016x})", x.to_bits());
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => format!("{i}i64"),
        Value::Float(x) => f(*x),
        Value::Vec2(v) => format!("V2 {{ x: {}, y: {} }}", f(v.x), f(v.y)),
        Value::Str(s) => format!("String::from({s:?})"),
        Value::Entity(e) => format!("{e}u64"),
        Value::Vec3(v) => format!("V3 {{ x: {}, y: {}, z: {} }}", f(v.x), f(v.y), f(v.z)),
        Value::Quat(v) => format!(
            "Quat {{ x: {}, y: {}, z: {}, w: {} }}",
            f(v.x),
            f(v.y),
            f(v.z),
            f(v.w)
        ),
        Value::Plane(v) => format!(
            "Plane {{ nx: {}, ny: {}, nz: {}, d: {} }}",
            f(v.nx),
            f(v.ny),
            f(v.nz),
            f(v.d)
        ),
        Value::List(e, items) => {
            let items: Vec<String> = items.iter().map(|x| wrap(e.ty(), &lit(x))).collect();
            format!("vec![{}]", items.join(", "))
        }
    }
}

fn wrap(ty: Ty, e: &str) -> String {
    let k = match ty {
        Ty::Bool => "Bool",
        Ty::Int => "Int",
        Ty::Float => "Float",
        Ty::Vec2 => "Vec2",
        Ty::Str => "Str",
        Ty::Entity => "Entity",
        Ty::Vec3 => "Vec3",
        Ty::Quat => "Quat",
        Ty::Plane => "Plane",
        Ty::List(el) => return format!("Value::List(rt::Elem::{}, {e})", el.variant()),
    };
    format!("Value::{k}({e})")
}

fn unwrap_method(ty: Ty) -> &'static str {
    match ty {
        Ty::Bool => "bool",
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Vec2 => "vec2",
        Ty::Str => "str",
        Ty::Entity => "entity",
        Ty::Vec3 => "vec3",
        Ty::Quat => "quat",
        Ty::Plane => "plane",
        Ty::List(_) => "list",
    }
}

fn copy(ty: Ty, e: String) -> String {
    if ty == Ty::Str || ty.is_list() {
        format!("{e}.clone()")
    } else {
        e
    }
}

fn ty_path(ty: Ty) -> String {
    format!(
        "rt::Ty::{}",
        match ty {
            Ty::Bool => "Bool",
            Ty::Int => "Int",
            Ty::Float => "Float",
            Ty::Vec2 => "Vec2",
            Ty::Str => "Str",
            Ty::Entity => "Entity",
            Ty::Vec3 => "Vec3",
            Ty::Quat => "Quat",
            Ty::Plane => "Plane",
            Ty::List(e) => return format!("rt::Ty::List(rt::Elem::{})", e.variant()),
        }
    )
}

struct Gen<'a> {
    p: &'a Program,
    /// Events that can call themselves; their calls save and restore parameters.
    reentrant: Vec<bool>,
}

impl Gen<'_> {
    fn expr(&self, src: &Src) -> String {
        match src {
            Src::Const(v) => lit(v),
            Src::Pin { node, pin } => self.pin(*node, *pin),
        }
    }

    fn pin(&self, node: u32, pin: u32) -> String {
        let n = &self.p.nodes[node as usize];
        if n.op.is_exec() {
            let slot = n.slots[pin as usize];
            return copy(self.p.slots[slot as usize], format!("self.s{slot}"));
        }
        let a = |i: usize| self.expr(&n.inputs[i]);
        let call = |f: &str, k: usize| {
            let args: Vec<String> = (0..k).map(a).collect();
            format!("ops::{f}({})", args.join(", "))
        };
        match &n.op {
            Op::Const(v) => lit(v),
            Op::Get(v) => {
                let d = &self.p.vars[*v as usize];
                copy(d.ty, format!("self.v_{}", d.name))
            }
            Op::SelfRef => "self.me".into(),
            Op::GameTime => "self.time".into(),
            Op::FrameDt => "self.dt".into(),
            Op::GetState => {
                "seal_graph::state_machine::current_name(&self.mem, self.machine.as_ref())".into()
            }
            Op::TimeInState => "self.mem.time_in_state".into(),
            Op::RandomInt => format!(
                "{{ let lo = {}; let hi = {}; rt::random_int(&mut self.rng, lo, hi) }}",
                a(0),
                a(1)
            ),
            Op::RandomFloat => format!(
                "{{ let lo = {}; let hi = {}; rt::random_float(&mut self.rng, lo, hi) }}",
                a(0),
                a(1)
            ),
            Op::Exists => format!("{{ let e = {}; host.exists(e) }}", a(0)),
            Op::IsA { prefab } => format!("{{ let e = {}; host.is_a(e, {prefab:?}) }}", a(0)),
            Op::FindFirst { prefab } => format!("rt::find_first(host, {prefab:?})"),
            Op::Phys(kind) => {
                let tys: &[Ty] = match kind {
                    crate::query::Kind::RaycastAll => &[Ty::Vec3, Ty::Vec3, Ty::Float, Ty::Int],
                    crate::query::Kind::ShapeCast => {
                        &[Ty::Str, Ty::Vec2, Ty::Vec3, Ty::Vec3, Ty::Float, Ty::Int]
                    }
                    crate::query::Kind::Point => &[Ty::Vec3, Ty::Int],
                    crate::query::Kind::OverlapBox => &[Ty::Vec3, Ty::Vec2, Ty::Int],
                    crate::query::Kind::OverlapCircle => &[Ty::Vec3, Ty::Float, Ty::Int],
                    crate::query::Kind::OverlapShape => &[
                        Ty::Str,
                        Ty::Vec2,
                        Ty::List(crate::rt::Elem::Vec2),
                        Ty::Vec3,
                        Ty::Float,
                        Ty::Int,
                    ],
                    crate::query::Kind::TestMove => &[Ty::Entity, Ty::Vec3, Ty::Int],
                };
                let args: Vec<String> = tys.iter().enumerate().map(|(i, ty)| wrap(*ty, &a(i))).collect();
                let method = unwrap_method(n.out_tys[pin as usize]);
                // Arguments may read the host themselves: evaluate them before the query borrows it.
                format!(
                    "{{ let args = [{}]; seal_graph::query::eval(seal_graph::query::Kind::{kind:?}, {pin}u32, host, &args).{method}() }}",
                    args.join(", ")
                )
            }
            Op::RayHit { dist } => {
                let pick = if *dist {
                    "hit.map_or(m, |h| h.1)"
                } else {
                    "hit.map_or(seal_graph::rt::NO_ENTITY, |h| h.0)"
                };
                format!("{{ let f = {}; let d = {}; let m: f64 = {}; let hit = host.ray_cast(f, d, m); {pick} }}", a(0), a(1), a(2))
            }
            Op::Player => format!("{{ let i = {}; host.player(i) }}", a(0)),
            Op::InputAxis => format!("{{ let n = {}; host.axis(&n) }}", a(0)),
            Op::ActionPressed => format!("{{ let n = {}; host.action_pressed(&n) }}", a(0)),
            Op::ActionHeld => format!("{{ let n = {}; host.action_held(&n) }}", a(0)),
            Op::ActionReleased => format!("{{ let n = {}; host.action_released(&n) }}", a(0)),
            Op::MouseDelta => "host.mouse_delta()".to_string(),
            Op::Device(kind) => kind.codegen().to_string(),
            Op::MouseWheel => "host.mouse_wheel()".to_string(),
            Op::PointerWorld => "host.pointer_world()".to_string(),
            Op::AiArrived => format!("{{ let e = {}; host.ai_arrived(e) }}", a(0)),
            Op::CharacterQuery(q) => match q {
                crate::rt::CharacterQuery::OnFloor => {
                    format!("{{ let e: u64 = {}; host.character_on_floor(e) }}", a(0))
                }
                crate::rt::CharacterQuery::OnWall => {
                    format!("{{ let e: u64 = {}; host.character_on_wall(e) }}", a(0))
                }
                crate::rt::CharacterQuery::Velocity => {
                    format!("{{ let e: u64 = {}; host.character_velocity(e) }}", a(0))
                }
            },
            Op::Score => format!("{{ let n: String = {}; host.score(&n) }}", a(0)),
            Op::UiValue => format!("{{ let n: String = {}; host.ui_value(&n) }}", a(0)),
            Op::UiInput => format!("{{ let n: String = {}; host.ui_input(&n) }}", a(0)),
            Op::GetString => format!(
                "{{ let t: String = {}; let k: String = {}; let v: String = {}; host.get_string(&t, &k, &v) }}",
                a(0),
                a(1),
                a(2)
            ),
            Op::GameState => "host.game_state()".to_string(),
            Op::HasSave => format!("{{ let n: String = {}; host.has_save(&n) }}", a(0)),
            Op::Saved => format!("{{ let n: String = {}; host.saved(&n) }}", a(0)),
            Op::SceneLoaded => format!("{{ let n: String = {}; host.scene_loaded(&n) }}", a(0)),
            Op::CurrentScene => "host.current_scene()".to_string(),
            Op::FindAll { prefab } => format!("rt::find_all(host, {prefab:?}).list()"),
            Op::BodiesInside => format!("rt::bodies_inside(host, {}).list()", a(0)),
            Op::Mass2d => format!("{{ let e = {}; rt::mass_2d(host, e) }}", a(0)),
            Op::WorldGet { component, field, ty } => format!(
                "rt::world_get(host, {}, {component:?}, {field:?}, {}, {})?.{}()",
                a(0),
                ty_path(*ty),
                n.id,
                unwrap_method(*ty)
            ),
            Op::Bin(b, t) => {
                let f = match (b, t) {
                    (BinOp::Add, Ty::Int) => "int_add",
                    (BinOp::Sub, Ty::Int) => "int_sub",
                    (BinOp::Mul, Ty::Int) => "int_mul",
                    (BinOp::Div, Ty::Int) => "int_div",
                    (BinOp::Mod, Ty::Int) => "int_mod",
                    (BinOp::Mod, _) => "float_mod",
                    (BinOp::Min, Ty::Int) => "int_min",
                    (BinOp::Max, Ty::Int) => "int_max",
                    (BinOp::Add, Ty::Float) => "float_add",
                    (BinOp::Sub, Ty::Float) => "float_sub",
                    (BinOp::Mul, Ty::Float) => "float_mul",
                    (BinOp::Div, Ty::Float) => "float_div",
                    (BinOp::Min, Ty::Float) => "float_min",
                    (BinOp::Max, Ty::Float) => "float_max",
                    (BinOp::Add, Ty::Vec2) => "vec2_add",
                    (BinOp::Sub, Ty::Vec2) => "vec2_sub",
                    (BinOp::And, _) => "and",
                    (BinOp::Or, _) => "or",
                    (BinOp::Concat, _) => "concat",
                    (BinOp::StrContains, _) => "str_contains",
                    (BinOp::StrStarts, _) => "str_starts",
                    (BinOp::StrEnds, _) => "str_ends",
                    (BinOp::Add, Ty::Vec3) => "vec3_add",
                    (BinOp::Sub, Ty::Vec3) => "vec3_sub",
                    (BinOp::Dot, Ty::Vec3) => "vec3_dot",
                    (BinOp::Dot, _) => "vec2_dot",
                    (BinOp::Cross, _) => "vec3_cross",
                    (BinOp::Pow, _) => "float_pow",
                    (BinOp::Atan2, _) => "float_atan2",
                    _ => unreachable!("checked by Program::build"),
                };
                call(f, 2)
            }
            Op::Un(u, t) => {
                let f = match (u, t) {
                    (UnOp::Neg, Ty::Int) => "int_neg",
                    (UnOp::Neg, Ty::Float) => "float_neg",
                    (UnOp::Neg, Ty::Vec2) => "vec2_neg",
                    (UnOp::Neg, Ty::Vec3) => "vec3_neg",
                    (UnOp::Abs, Ty::Int) => "int_abs",
                    (UnOp::Abs, Ty::Float) => "float_abs",
                    (UnOp::Sqrt, _) => "float_sqrt",
                    (UnOp::StrLen, _) => "str_len",
                    (UnOp::StrUpper, _) => "str_upper",
                    (UnOp::StrLower, _) => "str_lower",
                    (UnOp::StrTrim, _) => "str_trim",
                    (UnOp::StrTrimStart, _) => "str_trim_start",
                    (UnOp::StrTrimEnd, _) => "str_trim_end",
                    (UnOp::ParseInt, _) => "parse_int",
                    (UnOp::ParseFloat, _) => "parse_float",
                    (UnOp::NumText, Ty::Int) => "int_grouped",
                    (UnOp::NumText, _) => "float_grouped",
                    (UnOp::FloatText, _) => "float_text",
                    (UnOp::VecText, Ty::Vec2) => "vec2_text",
                    (UnOp::VecText, _) => "vec3_text",
                    (UnOp::Not, _) => "not",
                    (UnOp::ToFloat, _) => "to_float",
                    (UnOp::ToInt, _) => "to_int",
                    (UnOp::ToStr, Ty::Bool) => "bool_str",
                    (UnOp::ToStr, Ty::Int) => "int_str",
                    (UnOp::ToStr, Ty::Float) => "float_str",
                    (UnOp::ToStr, Ty::Vec2) => "vec2_str",
                    (UnOp::ToStr, Ty::Str) => "str_str",
                    (UnOp::ToStr, Ty::Entity) => "entity_str",
                    (UnOp::ToStr, Ty::Vec3) => "vec3_str",
                    (UnOp::Vec3X, _) => "vec3_x",
                    (UnOp::Vec3Y, _) => "vec3_y",
                    (UnOp::Vec3Z, _) => "vec3_z",
                    (UnOp::Length, Ty::Vec3) => "vec3_length",
                    (UnOp::Normalize, Ty::Vec3) => "vec3_normalize",
                    (UnOp::Vec2X, _) => "vec2_x",
                    (UnOp::Vec2Y, _) => "vec2_y",
                    (UnOp::Length, _) => "vec2_length",
                    (UnOp::Normalize, _) => "vec2_normalize",
                    (UnOp::Sin, _) => "float_sin",
                    (UnOp::Exp, _) => "float_exp",
                    (UnOp::Ln, _) => "float_ln",
                    (UnOp::Atan, _) => "float_atan",
                    (UnOp::Asin, _) => "float_asin",
                    (UnOp::Acos, _) => "float_acos",
                    (UnOp::Cos, _) => "float_cos",
                    (UnOp::Floor, _) => "floor_int",
                    (UnOp::Round, _) => "round_int",
                    (UnOp::Ceil, _) => "ceil_int",
                    _ => unreachable!("checked by Program::build"),
                };
                call(f, 1)
            }
            Op::Compare(c, _) => {
                let f = match c {
                    CmpOp::Lt => "lt",
                    CmpOp::Le => "le",
                    CmpOp::Gt => "gt",
                    CmpOp::Ge => "ge",
                    CmpOp::Eq => "eq",
                    CmpOp::Ne => "ne",
                };
                call(f, 2)
            }
            Op::Select(_) => call("select", 3),
            Op::MakeVec2 => call("vec2_make", 2),
            Op::MakeVec3 => call("vec3_make", 3),
            Op::Scale(Ty::Vec3) => call("vec3_scale", 2),
            Op::Scale(_) => call("vec2_scale", 2),
            Op::List(l, e) => {
                let item = |i: usize| wrap(e.ty(), &a(i));
                match l {
                    ListOp::Make(k) => {
                        let items: Vec<String> = (0..*k as usize).map(item).collect();
                        format!("vec![{}]", items.join(", "))
                    }
                    ListOp::Len => format!("ops::list_len({})", a(0)),
                    ListOp::Get => format!(
                        "ops::list_get({}, {}, {}).{}()",
                        a(0),
                        a(1),
                        wrap(e.ty(), &lit(&e.ty().zero())),
                        unwrap_method(e.ty())
                    ),
                    ListOp::Push => format!("ops::list_push({}, {})", a(0), item(1)),
                    ListOp::Set => format!("ops::list_set({}, {}, {})", a(0), a(1), item(2)),
                    ListOp::Insert => format!("ops::list_insert({}, {}, {})", a(0), item(1), a(2)),
                    ListOp::RemoveAt => format!("ops::list_remove_at({}, {})", a(0), a(1)),
                    ListOp::RemoveItem => format!("ops::list_remove_item({}, {})", a(0), item(1)),
                    ListOp::Find => format!("ops::list_find({}, {})", a(0), item(1)),
                    ListOp::Contains => format!("ops::list_contains({}, {})", a(0), item(1)),
                }
            }
            Op::DialogAnswer => {
                if pin == 0 {
                    "host.dialog_button()".into()
                } else {
                    "host.dialog_text()".into()
                }
            }
            Op::CloudJob => match pin {
                0 => "host.cloud_kind()".into(),
                1 => "host.cloud_slot()".into(),
                2 => "host.cloud_saved_at()".into(),
                _ => "host.cloud_body()".into(),
            },
            Op::Server(crate::ServerCall::Position) => format!(
                "{{ let args = [{}, {}]; rt::physics_server(host, \"position\", &args, {})?.vec3() }}",
                wrap(Ty::Str, &a(0)),
                wrap(Ty::Float, &a(1)),
                n.id
            ),
            Op::Poly(kind) => {
                let f = match kind {
                    crate::PolyOp::Offset => "offset_values",
                    crate::PolyOp::Merge => "merge_values",
                    crate::PolyOp::Clip => "clip_values",
                    crate::PolyOp::Intersect => "intersect_values",
                };
                format!("seal_graph::poly2d::{f}({}, {})", a(0), a(1))
            }
            Op::ScriptCore(op) => {
                let args: Vec<String> = (0..n.inputs.len()).map(|i| a(i)).collect();
                format!(
                    "{{ let args = vec![{}]; seal_graph::script_core::eval_core(seal_graph::script_core::ScriptCoreOp::{op:?}, &args, host) }}",
                    args.join(", ")
                )
            }
            _ => unreachable!("execution nodes are read through slots"),
        }
    }

    fn push(t: Option<u32>) -> String {
        match t {
            Some(t) => format!("stack.push(Cont::Exec({t}));"),
            None => String::new(),
        }
    }

    fn arms(&self, out: &mut String) {
        for (i, n) in self.p.nodes.iter().enumerate() {
            let i = i as u32;
            let id = n.id;
            let push = Self::push;
            match &n.op {
                Op::Event { .. } => {}
                Op::Branch => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; if {} {{ {} }} else {{ {} }} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0]),
                        push(n.next[1])
                    );
                }
                Op::Sequence(count) => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; stack.push(Cont::Seq {{ node: {i}, next: 0 }}); }}"
                    );
                    let _ = writeln!(out, "                Cont::Seq {{ node: {i}, next }} => {{ budget.step({id})?; match next {{");
                    for k in 0..*count {
                        let more = if k + 1 < *count {
                            format!("stack.push(Cont::Seq {{ node: {i}, next: {} }});", k + 1)
                        } else {
                            String::new()
                        };
                        let _ = writeln!(
                            out,
                            "                    {k} => {{ {more} {} }}",
                            push(n.next[k as usize])
                        );
                    }
                    let _ = writeln!(out, "                    _ => {{}}\n                }} }}");
                }
                Op::For => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let first: i64 = {}; let last: i64 = {}; stack.push(Cont::Loop {{ node: {i}, next: first, last }}); }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1])
                    );
                    let slot = n.slots[0];
                    let _ = writeln!(
                        out,
                        "                Cont::Loop {{ node: {i}, next, last }} => {{ budget.step({id})?; if next <= last && !({stop}) {{ self.s{slot} = next; if next < last {{ stack.push(Cont::Loop {{ node: {i}, next: next + 1, last }}); }} else {{ {done} }} {body} }} else {{ {done} }} }}",
                        done = push(n.next[1]),
                        body = push(n.next[0]),
                        stop = self.expr(&n.inputs[2])
                    );
                }
                Op::Delay => {
                    let sched = match n.next[0] {
                        Some(t) => format!("self.timers.schedule({i}, self.time + s, {t});"),
                        None => "let _ = s;".into(),
                    };
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: f64 = {}; {sched} }}",
                        self.expr(&n.inputs[0])
                    );
                }
                Op::Set(v) => {
                    let d = &self.p.vars[*v as usize];
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let val: {} = {}; self.v_{} = val.clone(); self.s{} = val; {} }}",
                        d.ty.rust(),
                        self.expr(&n.inputs[0]),
                        d.name,
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::Call(e) => {
                    let ev = &self.p.nodes[self.p.events[*e as usize].node as usize];
                    let mut body = String::new();
                    for (k, s) in n.inputs.iter().enumerate() {
                        let _ = write!(body, "let a{k} = {}; ", self.expr(s));
                    }
                    let frame = self.reentrant[*e as usize] && !ev.slots.is_empty();
                    let mut restore = String::new();
                    if frame {
                        let saved: Vec<String> =
                            ev.slots.iter().map(|s| format!("self.s{s}.clone(), ")).collect();
                        let _ = write!(body, "f{e}.push(({})); ", saved.concat());
                        let names: Vec<String> =
                            (0..ev.slots.len()).map(|k| format!("x{k}, ")).collect();
                        let sets: Vec<String> = ev
                            .slots
                            .iter()
                            .enumerate()
                            .map(|(k, s)| format!("self.s{s} = x{k}; "))
                            .collect();
                        restore = format!(
                            "if let Some(({})) = f{e}.pop() {{ {}}} ",
                            names.concat(),
                            sets.concat()
                        );
                    }
                    for (k, slot) in ev.slots.iter().enumerate() {
                        let _ = write!(body, "self.s{slot} = a{k}; ");
                    }
                    for slot in &n.slots {
                        let _ = write!(
                            body,
                            "self.s{slot} = {}; ",
                            lit(&self.p.slots[*slot as usize].zero())
                        );
                    }
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; {body}stack.push(Cont::Ret {{ node: {i} }}); {} }}",
                        push(ev.next[0])
                    );
                    let _ = writeln!(
                        out,
                        "                Cont::Ret {{ node: {i} }} => {{ budget.step({id})?; {restore}{} }}",
                        push(n.next[0])
                    );
                }
                Op::Return(e) => {
                    let mut body = String::new();
                    for (k, s) in n.inputs.iter().enumerate() {
                        let _ = write!(body, "let r{k} = {}; ", self.expr(s));
                    }
                    let mut fill = String::new();
                    for (c, call) in self.p.nodes.iter().enumerate() {
                        if call.op == Op::Call(*e) {
                            let _ = write!(fill, "{c} => {{ ");
                            for (k, slot) in call.slots.iter().enumerate() {
                                let _ = write!(fill, "self.s{slot} = r{k}.clone(); ");
                            }
                            fill.push_str("} ");
                        }
                    }
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; {body}while let Some(&top) = stack.last() {{ if let Cont::Ret {{ node }} = top {{ match node {{ {fill}_ => {{}} }} break; }} stack.pop(); }} }}"
                    );
                }
                Op::SetTimer(e) => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let secs: f64 = {}; let looping: bool = {}; self.timers.set_event({i}, {e}, self.time, secs, looping); {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::ClearTimer(e) => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; self.timers.clear_event({e}); {} }}",
                        push(n.next[0])
                    );
                }
                Op::Log => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let m: String = {}; host.log(&m); {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::Spawn { prefab } => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; self.s{} = rt::spawn(host, {prefab:?}, {id})?; {} }}",
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::Despawn => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; rt::despawn(host, e, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::UiShow => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let w: String = {}; let v: bool = {}; rt::ui_show(host, &w, v, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::UiText => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: String = {}; let t: String = {}; rt::ui_text(host, &e, &t, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::Typewriter => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: String = {}; let s: bool = {}; host.typewriter(&e, s).map_err(|m| seal_graph::GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::PlaySequence => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let p: String = {}; rt::sequence(host, &p, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::PlayAnim => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let a: String = {}; rt::anim(host, e, Some(&a), {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::Ai(kind) => {
                    let order = match kind {
                        crate::rt::AiKind::MoveTo => format!(
                            "rt::AiOrder::MoveTo {{ target: {}, speed: {} }}",
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2])
                        ),
                        crate::rt::AiKind::Chase => format!(
                            "rt::AiOrder::Chase {{ target: {}, speed: {} }}",
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2])
                        ),
                        crate::rt::AiKind::Patrol => format!(
                            "rt::AiOrder::Patrol {{ a: {}, b: {}, speed: {} }}",
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2]),
                            self.expr(&n.inputs[3])
                        ),
                        crate::rt::AiKind::Stop => "rt::AiOrder::Stop".to_string(),
                    };
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let o = {order}; rt::ai_order(host, e, o, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::Game(kind) => {
                    let cmd = match kind {
                        crate::rt::GameKind::Possess => format!(
                            "rt::GameCmd::Possess {{ index: {}, entity: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::SpawnPlayer => format!(
                            "rt::GameCmd::SpawnPlayer {{ index: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::AddScore => format!(
                            "rt::GameCmd::AddScore {{ name: {}, amount: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::SetState => format!(
                            "rt::GameCmd::SetState {{ state: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::SaveGame => {
                            format!("rt::GameCmd::SaveGame {{ slot: {} }}", self.expr(&n.inputs[0]))
                        }
                        crate::rt::GameKind::LoadGame => {
                            format!("rt::GameCmd::LoadGame {{ slot: {} }}", self.expr(&n.inputs[0]))
                        }
                        crate::rt::GameKind::DeleteSave => format!(
                            "rt::GameCmd::DeleteSave {{ slot: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::SetSaved => format!(
                            "rt::GameCmd::SetSaved {{ key: {}, value: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::OpenScene => {
                            format!(
                                "rt::GameCmd::OpenScene {{ path: {} }}",
                                self.expr(&n.inputs[0])
                            )
                        }
                        crate::rt::GameKind::AddScene => {
                            format!("rt::GameCmd::AddScene {{ path: {} }}", self.expr(&n.inputs[0]))
                        }
                        crate::rt::GameKind::LoadSceneAsync => format!(
                            "rt::GameCmd::LoadSceneAsync {{ path: {}, additive: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::UnloadScene => format!(
                            "rt::GameCmd::UnloadScene {{ path: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::KeepAcrossScenes => format!(
                            "rt::GameCmd::KeepAcrossScenes {{ entity: {}, keep: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::SetWindowMode => format!(
                            "rt::GameCmd::SetWindowMode {{ mode: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::OpenGameWindow => format!(
                            "rt::GameCmd::OpenGameWindow {{ id: {}, title: {}, width: {} as u32, height: {} as u32, x: {} as i32, y: {} as i32, camera_entity: {}, ui_screen: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2]),
                            self.expr(&n.inputs[3]),
                            self.expr(&n.inputs[4]),
                            self.expr(&n.inputs[5]),
                            self.expr(&n.inputs[6]),
                            self.expr(&n.inputs[7]),
                        ),
                        crate::rt::GameKind::MoveGameWindow => format!(
                            "rt::GameCmd::MoveGameWindow {{ id: {} as u32, x: {} as i32, y: {} as i32 }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2]),
                        ),
                        crate::rt::GameKind::ResizeGameWindow => format!(
                            "rt::GameCmd::ResizeGameWindow {{ id: {} as u32, width: {} as u32, height: {} as u32 }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1]),
                            self.expr(&n.inputs[2]),
                        ),
                        crate::rt::GameKind::CloseGameWindow => format!(
                            "rt::GameCmd::CloseGameWindow {{ id: {} as u32 }}",
                            self.expr(&n.inputs[0])
                        ),
                        crate::rt::GameKind::RequestAttention => {
                            "rt::GameCmd::RequestAttention".to_string()
                        }
                        crate::rt::GameKind::Spectate => format!(
                            "rt::GameCmd::Spectate {{ entity: {}, on: {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1])
                        ),
                        crate::rt::GameKind::Unpossess => format!(
                            "rt::GameCmd::Unpossess {{ index: {} }}",
                            self.expr(&n.inputs[0])
                        ),
                    };
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let c = {cmd}; rt::game(host, c, {id})?; {} }}",
                        push(n.next[0])
                    );
                }
                Op::PlayClip => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let a: String = format!(\"clip:{{}}\", {}); rt::anim(host, e, Some(&a), {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::StopAnim => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; rt::anim(host, e, None, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::ApplyImpulse => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let v = {}; rt::apply_impulse(host, e, v, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::Character(kind) => {
                    let arm = match kind {
                        crate::rt::CharacterKind::Move => format!(
                            "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let w = {}; rt::character_move(host, e, w, {id})?; {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1]),
                            push(n.next[0])
                        ),
                        crate::rt::CharacterKind::Jump => format!(
                            "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; rt::character_jump(host, e, {id})?; {} }}",
                            self.expr(&n.inputs[0]),
                            push(n.next[0])
                        ),
                        crate::rt::CharacterKind::SetMode => format!(
                            "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let m: String = {}; rt::character_mode(host, e, &m, {id})?; {} }}",
                            self.expr(&n.inputs[0]),
                            self.expr(&n.inputs[1]),
                            push(n.next[0])
                        ),
                    };
                    let _ = writeln!(out, "{arm}");
                }
                Op::ApplyForce2d => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let f = {}; rt::apply_force_2d(host, e, f, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::ApplyTorque2d => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let t: f64 = {}; rt::apply_torque_2d(host, e, t, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::SetVelocity2d => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let v = {}; let a: f64 = {}; rt::set_velocity_2d(host, e, v, a, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        push(n.next[0])
                    );
                }
                Op::SetColliderShape => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let shape: String = {}; let size = {}; let points = {}; rt::set_collider_shape(host, e, &shape, size, &points, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        self.expr(&n.inputs[3]),
                        push(n.next[0])
                    );
                }
                Op::SetColliderEnabled => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let on: bool = {}; rt::set_collider_enabled(host, e, on, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::SetJointTarget2d => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let t = {}; rt::set_joint_target_2d(host, e, t, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::CameraShake => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: f64 = {}; let d: f64 = {}; rt::camera_shake(host, s, d, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::Rumble => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: f64 = {}; let w: f64 = {}; let d: f64 = {}; rt::rumble(host, s, w, d, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        push(n.next[0])
                    );
                }
                Op::Device(crate::device_nodes::DeviceKind::SetLight) => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let r: f64 = {}; let g: f64 = {}; let b: f64 = {}; rt::set_pad_light(host, r, g, b, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        push(n.next[0])
                    );
                }
                Op::RebindAction => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let a: String = {}; let slot: i64 = {}; rt::rebind_action(host, &a, slot, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        push(n.next[0])
                    );
                }
                Op::PlayMusic => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let f: String = {}; let looping: bool = {}; let fade: f64 = {}; let start: f64 = {}; rt::play_music(host, &f, looping, fade, start, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        self.expr(&n.inputs[3]),
                        push(n.next[0])
                    );
                }
                Op::StopMusic => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let fade: f64 = {}; rt::stop_music(host, fade, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::SetMusicVolume => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let v: f64 = {}; rt::set_music_volume(host, v, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::PlaySound { at } => {
                    let l = if *at {
                        format!("Some({})", self.expr(&n.inputs[3]))
                    } else {
                        "None".into()
                    };
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: String = {}; let v: f64 = {}; let p: f64 = {}; rt::play_sound(host, &s, v, p, {l}, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        push(n.next[0])
                    );
                }
                Op::SetState => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let name: String = {}; let machine = self.machine.clone().ok_or_else(|| GraphError::at({id}, \"this graph has no state machine\"))?; let plan = seal_graph::state_machine::plan_name(&self.mem, &machine, &name, {id})?; {} stack.push(Cont::Commit {{ node: {i}, state: plan.index, forced: true, enter: plan.enter }}); if let Some(ev) = plan.exit {{ self.push_event(&mut stack, ev); }} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::Trigger => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let name: String = {}; seal_graph::state_machine::note(&mut self.mem, &name); {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::OpenFileDialog => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let t: String = {}; let f: String = {}; self.s{} = host.open_file_dialog(&t, &f).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::SaveFileDialog => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let t: String = {}; let name: String = {}; self.s{} = host.save_file_dialog(&t, &name).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::Encrypt => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let t: String = {}; self.s{} = host.encrypt_text(&t).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::Decrypt => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let t: String = {}; self.s{} = host.decrypt_text(&t).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        n.slots[0],
                        push(n.next[0])
                    );
                }
                Op::ShowDialog => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let k: String = {}; let t: String = {}; let m: String = {}; let p: String = {}; let theme: String = {}; host.show_dialog(&k, &t, &m, &p, &theme).map_err(|e| GraphError::at({id}, e))?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        self.expr(&n.inputs[3]),
                        self.expr(&n.inputs[4]),
                        push(n.next[0])
                    );
                }
                Op::CloudUpload => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: String = {}; host.cloud_upload(&s).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::CloudDownload => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: String = {}; host.cloud_download(&s).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        push(n.next[0])
                    );
                }
                Op::CloudFlush => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; host.cloud_flush().map_err(|m| GraphError::at({id}, m))?; {} }}",
                        push(n.next[0])
                    );
                }
                Op::CloudReply => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let s: String = {}; let at: i64 = {}; let b: String = {}; host.cloud_reply(&s, at, &b).map_err(|m| GraphError::at({id}, m))?; {} }}",
                        self.expr(&n.inputs[0]),
                        self.expr(&n.inputs[1]),
                        self.expr(&n.inputs[2]),
                        push(n.next[0])
                    );
                }
                Op::WorldSet { component, field, ty } => {
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let e: u64 = {}; let v = {}; rt::world_set(host, e, {component:?}, {field:?}, v, {id})?; {} }}",
                        self.expr(&n.inputs[0]),
                        wrap(*ty, &self.expr(&n.inputs[1])),
                        push(n.next[0])
                    );
                }
                Op::Server(call) if call.is_exec() => {
                    let tys: &[Ty] = match call {
                        crate::ServerCall::Body => {
                            &[Ty::Str, Ty::Float, Ty::Float, Ty::Float, Ty::Bool]
                        }
                        crate::ServerCall::Shape => &[
                            Ty::Str,
                            Ty::Float,
                            Ty::Str,
                            Ty::Float,
                            Ty::Float,
                            Ty::Float,
                            Ty::Str,
                        ],
                        crate::ServerCall::Joint => {
                            &[Ty::Str, Ty::Str, Ty::Float, Ty::Float, Ty::Float]
                        }
                        crate::ServerCall::Step => &[Ty::Str, Ty::Float],
                        crate::ServerCall::Position => &[],
                    };
                    let args: Vec<String> = tys
                        .iter()
                        .enumerate()
                        .map(|(k, ty)| wrap(*ty, &self.expr(&n.inputs[k])))
                        .collect();
                    let store = match n.slots.first() {
                        Some(slot) => format!("self.s{slot} = "),
                        None => String::new(),
                    };
                    let _ = writeln!(
                        out,
                        "                Cont::Exec({i}) => {{ budget.step({id})?; let args = [{}]; {store}rt::physics_server(host, \"{}\", &args, {id})?; {} }}",
                        args.join(", "),
                        call.name(),
                        push(n.next[0])
                    );
                }
                _ => {}
            }
        }
    }
}

fn machine_value(p: &Program) -> String {
    use crate::state_machine::CondProg;
    let Some(m) = &p.machine else { return "None".into() };
    let opt = |e: Option<u32>| match e {
        Some(i) => format!("Some({i})"),
        None => "None".into(),
    };
    let states: Vec<String> = m
        .states
        .iter()
        .map(|s| {
            format!(
                "seal_graph::state_machine::StateProg {{ name: String::from({:?}), enter: {}, update: {}, exit: {} }}",
                s.name,
                opt(s.enter),
                opt(s.update),
                opt(s.exit)
            )
        })
        .collect();
    let transitions: Vec<String> = m
        .transitions
        .iter()
        .map(|t| {
            let cond = match &t.cond {
                CondProg::BoolVar(i) => format!("seal_graph::state_machine::CondProg::BoolVar({i})"),
                CondProg::Compare { var, op, value } => {
                    let op = match op {
                        crate::CmpOp::Lt => "Lt",
                        crate::CmpOp::Le => "Le",
                        crate::CmpOp::Gt => "Gt",
                        crate::CmpOp::Ge => "Ge",
                        crate::CmpOp::Eq => "Eq",
                        crate::CmpOp::Ne => "Ne",
                    };
                    format!(
                        "seal_graph::state_machine::CondProg::Compare {{ var: {var}, op: seal_graph::CmpOp::{op}, value: {} }}",
                        wrap(value.ty(), &lit(value))
                    )
                }
                CondProg::Event(name) => {
                    format!("seal_graph::state_machine::CondProg::Event(String::from({name:?}))")
                }
                CondProg::After(seconds) => format!(
                    "seal_graph::state_machine::CondProg::After(f64::from_bits(0x{:016x}))",
                    seconds.to_bits()
                ),
            };
            format!(
                "seal_graph::state_machine::TransProg {{ from: {}, to: {}, priority: {}, cond: {cond} }}",
                t.from, t.to, t.priority
            )
        })
        .collect();
    format!(
        "Some(seal_graph::state_machine::MachineProg {{ initial: {}, states: vec![{}], transitions: vec![{}] }})",
        m.initial,
        states.join(", "),
        transitions.join(", ")
    )
}

fn commit_arm(p: &Program) -> String {
    let ids: Vec<String> =
        p.nodes.iter().enumerate().map(|(i, n)| format!("{i} => {}", n.id)).collect();
    let id_match = if ids.is_empty() {
        "0".into()
    } else {
        format!("match node {{ {}, _ => 0 }}", ids.join(", "))
    };
    format!(
        "Cont::Commit {{ node, state, forced, enter }} => {{ let id = {id_match}; budget.step(id)?; seal_graph::state_machine::commit(&mut self.mem, state, forced); if let Some(ev) = enter {{ self.push_event(&mut stack, ev); }} }}"
    )
}

fn machine_methods(p: &Program) -> String {
    let mut push_arms = String::new();
    let mut fire_arms = String::new();
    for (k, e) in p.events.iter().enumerate() {
        if e.params.is_empty() {
            if let Some(t) = p.nodes[e.node as usize].next.first().copied().flatten() {
                let _ = writeln!(push_arms, "            {k} => stack.push(Cont::Exec({t})),");
            }
            let _ = writeln!(fire_arms, "            {k} => self.event_{k}(&[], host),");
        }
    }
    let vars = if p.vars.is_empty() {
        "Vec::new()".to_string()
    } else {
        let items: Vec<String> =
            p.vars.iter().map(|v| wrap(v.ty, &copy(v.ty, format!("self.v_{}", v.name)))).collect();
        format!("vec![{}]", items.join(", "))
    };
    format!(
        "        fn push_event(&self, stack: &mut Vec<Cont>, ev: u32) {{\n            match ev {{\n{push_arms}                _ => {{}}\n            }}\n        }}\n\n        fn fire_event(&mut self, ev: u32, host: &mut dyn Host) -> Result<(), GraphError> {{\n            match ev {{\n{fire_arms}                _ => Ok(()),\n            }}\n        }}\n\n        fn apply_switch(&mut self, plan: seal_graph::state_machine::Switch, forced: bool, host: &mut dyn Host) -> Result<(), GraphError> {{\n            if let Some(ev) = plan.exit {{\n                self.fire_event(ev, host)?;\n            }}\n            seal_graph::state_machine::commit(&mut self.mem, plan.index, forced);\n            if let Some(ev) = plan.enter {{\n                self.fire_event(ev, host)?;\n            }}\n            Ok(())\n        }}\n\n        fn step_machine(&mut self, host: &mut dyn Host) -> Result<(), GraphError> {{\n            let Some(machine) = self.machine.clone() else {{ return Ok(()); }};\n            if seal_graph::state_machine::begin_step(&mut self.mem, self.dt) {{\n                let plan = seal_graph::state_machine::plan_index(&self.mem, &machine, machine.initial)?;\n                self.apply_switch(plan, false, host)?;\n            }}\n            let update = self.mem.current.and_then(|i| machine.states.get(i as usize).and_then(|s| s.update));\n            if let Some(ev) = update {{\n                self.fire_event(ev, host)?;\n            }}\n            let vars = {vars};\n            if let Some(plan) = seal_graph::state_machine::finish(&mut self.mem, &machine, &vars)? {{\n                self.apply_switch(plan, false, host)?;\n            }}\n            Ok(())\n        }}\n"
    )
}

/// Rust source for one graph, as a module named after it.
pub fn compile(p: &Program) -> String {
    let g = Gen { p, reentrant: p.reentrant() };
    let m = module_name(&p.name);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "/// Compiled from graph {:?} by seal-graph. Edit the graph, not this file.",
        p.name
    );
    let _ = writeln!(out, "#[allow(warnings, clippy::all, clippy::pedantic)]");
    let _ = writeln!(out, "pub mod {m} {{");
    let _ = writeln!(
        out,
        "    use seal_graph::rt::{{self, ops, Budget, Cont, GraphError, GraphInstance, Host, RunState, Timers, V2, V3, Value}};\n"
    );
    let _ = writeln!(out, "    pub const NAME: &str = {:?};", p.name);
    let names: Vec<String> = p.events.iter().map(|e| format!("{:?}", e.name)).collect();
    let _ = writeln!(out, "    pub const EVENTS: &[&str] = &[{}];\n", names.join(", "));
    let _ = writeln!(out, "    #[derive(Clone, Debug)]\n    pub struct Graph {{");
    let _ = writeln!(out, "        pub me: u64,\n        time: f64,\n        dt: f64,\n        rng: u64,\n        timers: Timers,\n        budget: u64,");
    let _ = writeln!(out, "        mem: seal_graph::state_machine::MachineMemory,");
    let _ = writeln!(out, "        machine: Option<seal_graph::state_machine::MachineProg>,");
    for v in &p.vars {
        let _ = writeln!(out, "        pub v_{}: {},", v.name, v.ty.rust());
    }
    for (k, t) in p.slots.iter().enumerate() {
        let _ = writeln!(out, "        s{k}: {},", t.rust());
    }
    let _ = writeln!(out, "    }}\n\n    impl Graph {{");
    let _ = writeln!(out, "        pub fn new(me: u64) -> Self {{");
    let _ = writeln!(out, "            Self {{\n                me,\n                time: 0.0,\n                dt: 0.0,\n                rng: rt::rng_seed(me),\n                timers: Timers::default(),\n                budget: rt::DEFAULT_BUDGET,");
    let _ =
        writeln!(out, "                mem: seal_graph::state_machine::MachineMemory::default(),");
    let _ = writeln!(out, "                machine: {},", machine_value(p));
    for v in &p.vars {
        let _ = writeln!(out, "                v_{}: {},", v.name, lit(&v.default));
    }
    for (k, t) in p.slots.iter().enumerate() {
        let _ = writeln!(out, "                s{k}: {},", lit(&t.zero()));
    }
    let _ = writeln!(out, "            }}\n        }}\n");
    let _ = writeln!(
        out,
        "        fn run(&mut self, mut stack: Vec<Cont>, host: &mut dyn Host) -> Result<(), GraphError> {{"
    );
    let _ = writeln!(out, "            let mut budget = Budget::new(self.budget);");
    for (e, info) in p.events.iter().enumerate() {
        if g.reentrant[e] && !p.nodes[info.node as usize].slots.is_empty() {
            let _ = writeln!(out, "            let mut f{e} = Vec::new();");
        }
    }
    let _ =
        writeln!(out, "            while let Some(c) = stack.pop() {{\n                match c {{");
    g.arms(&mut out);
    let _ = writeln!(out, "                {}", commit_arm(p));
    let _ = writeln!(out, "                _ => unreachable!(\"not a node of this graph\"),");
    let _ = writeln!(out, "                }}\n            }}\n            Ok(())\n        }}\n");
    // One start function per event.
    for (k, e) in p.events.iter().enumerate() {
        let node = &p.nodes[e.node as usize];
        let _ = writeln!(out, "        fn event_{k}(&mut self, args: &[Value], host: &mut dyn Host) -> Result<(), GraphError> {{");
        for (j, (slot, (_, ty))) in node.slots.iter().zip(&e.params).enumerate() {
            let _ = writeln!(out, "            self.s{slot} = args[{j}].{}();", unwrap_method(*ty));
        }
        match node.next[0] {
            Some(t) => {
                let _ = writeln!(out, "            self.run(vec![Cont::Exec({t})], host)");
            }
            None => {
                let _ = writeln!(out, "            Ok(())");
            }
        }
        let _ = writeln!(out, "        }}\n");
    }
    out.push_str(&machine_methods(p));
    let _ = writeln!(out, "    }}\n\n    impl GraphInstance for Graph {{");
    let _ = writeln!(
        out,
        "        fn fire(&mut self, event: &str, args: &[Value], host: &mut dyn Host) -> Result<RunState, GraphError> {{\n            match event {{"
    );
    for (k, e) in p.events.iter().enumerate() {
        let tys: Vec<String> = e.params.iter().map(|(_, t)| ty_path(*t)).collect();
        let _ = writeln!(
            out,
            "                {:?} => {{ rt::check_args(event, &[{}], args)?; if self.machine.is_some() {{ seal_graph::state_machine::note(&mut self.mem, event); }} self.event_{k}(args, host)?; }}",
            e.name,
            tys.join(", ")
        );
    }
    let _ = writeln!(
        out,
        "                _ => {{ if self.machine.is_some() {{ seal_graph::state_machine::note(&mut self.mem, event); }} }}\n            }}\n            Ok(RunState::Done)\n        }}\n"
    );
    let _ = writeln!(
        out,
        "        fn advance(&mut self, dt: f64, host: &mut dyn Host) -> Result<RunState, GraphError> {{"
    );
    let _ = writeln!(out, "            self.time += dt;\n            self.dt = dt;");
    let mut arms = String::new();
    for (k, e) in p.events.iter().enumerate() {
        if e.params.is_empty() {
            let _ = write!(
                arms,
                "{k} => {{ if self.machine.is_some() {{ seal_graph::state_machine::note(&mut self.mem, {:?}); }} self.event_{k}(&[], host)?; }}, ",
                e.name
            );
        }
    }
    let _ = writeln!(out, "            for t in self.timers.take_due(self.time) {{\n                if !t.event {{\n                    self.run(vec![Cont::Exec(t.target)], host)?;\n                }} else if self.timers.live(&t) {{\n                    match t.target {{ {arms}_ => {{}} }}\n                }}\n            }}");
    if let Some(k) = p.event("tick") {
        let _ = writeln!(
            out,
            "            if self.machine.is_some() {{ seal_graph::state_machine::note(&mut self.mem, \"tick\"); }}\n            self.event_{k}(&[Value::Float(dt)], host)?;"
        );
    }
    let _ = writeln!(out, "            self.step_machine(host)?;");
    let _ = writeln!(out, "            Ok(RunState::Done)\n        }}\n");
    let vars: Vec<String> = p
        .vars
        .iter()
        .map(|v| {
            format!(
                "({:?}.to_string(), {})",
                v.name,
                wrap(v.ty, &copy(v.ty, format!("self.v_{}", v.name)))
            )
        })
        .collect();
    let _ = writeln!(
        out,
        "        fn vars(&self) -> Vec<(String, Value)> {{\n            vec![{}]\n        }}\n",
        vars.join(", ")
    );
    let _ = writeln!(out, "        fn time(&self) -> f64 {{\n            self.time\n        }}\n");
    let _ = writeln!(
        out,
        "        fn pending_delays(&self) -> usize {{\n            self.timers.len()\n        }}\n"
    );
    let _ = writeln!(out, "        fn set_budget(&mut self, steps: u64) {{\n            self.budget = steps;\n        }}");
    let _ = writeln!(out, "    }}\n}}");
    out
}

/// Several graphs in one file, plus `create(name, me)` that builds any of them.
pub fn compile_all(programs: &[Program]) -> String {
    let mut out = String::new();
    for p in programs {
        out.push_str(&compile(p));
        out.push('\n');
    }
    let _ = writeln!(out, "/// Every compiled graph by name.");
    let names: Vec<String> = programs.iter().map(|p| format!("{:?}", p.name)).collect();
    let _ = writeln!(out, "pub const GRAPHS: &[&str] = &[{}];\n", names.join(", "));
    let _ = writeln!(out, "/// Build a compiled graph by name, running on entity `me`.");
    let _ = writeln!(
        out,
        "pub fn create(name: &str, me: u64) -> Option<Box<dyn seal_graph::rt::GraphInstance>> {{\n    match name {{"
    );
    for p in programs {
        let _ = writeln!(
            out,
            "        {:?} => Some(Box::new({}::Graph::new(me))),",
            p.name,
            module_name(&p.name)
        );
    }
    let _ = writeln!(out, "        _ => None,\n    }}\n}}");
    out
}
