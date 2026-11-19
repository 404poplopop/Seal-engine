//! Graph nodes for data assets, CSV, and JSON.

use seal_graph::{Graph, Host, Interp, Node, Op, Program, Ty, Value};

struct DataHost {
    root: std::path::PathBuf,
}

fn type_path(data_text: &str) -> Option<String> {
    for raw in data_text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with("type") {
            return line.split_once('=').map(|(_, v)| v.trim().to_string());
        }
    }
    None
}

fn field_value(data_text: &str, field: &str) -> String {
    for raw in data_text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("value") {
            let rest = rest.trim();
            if let Some((name, val)) = rest.split_once('=') {
                if name.trim() == field {
                    return val.trim().to_string();
                }
            }
        }
    }
    String::new()
}

fn csv_cell(text: &str, row: usize, col: usize) -> String {
    let rows: Vec<Vec<&str>> =
        text.lines().map(|l| l.split(',').map(str::trim).collect()).collect();
    rows.get(row).and_then(|r| r.get(col)).map(|s| s.to_string()).unwrap_or_default()
}

fn json_value(text: &str, path: &str) -> String {
    let needle = format!("\"{path}\":");
    let Some(i) = text.find(&needle) else {
        return String::new();
    };
    let rest = text[i + needle.len()..].trim_start();
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    rest[..end].trim().trim_matches('"').to_string()
}

impl Host for DataHost {
    fn get(
        &mut self,
        _entity: u64,
        _component: &str,
        _field: &str,
        _ty: Ty,
    ) -> Result<Value, String> {
        Err("no components".into())
    }

    fn set(
        &mut self,
        _entity: u64,
        _component: &str,
        _field: &str,
        _value: Value,
    ) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, _message: &str) {}

    fn data_field(&mut self, path: &str, field: &str) -> String {
        let Ok(text) = std::fs::read_to_string(self.root.join(path)) else {
            return String::new();
        };
        let _ = type_path(&text);
        field_value(&text, field)
    }

    fn read_csv_cell(&mut self, path: &str, row: i64, col: i64) -> String {
        let Ok(text) = std::fs::read_to_string(self.root.join(path)) else {
            return String::new();
        };
        csv_cell(&text, row as usize, col as usize)
    }

    fn read_json_value(&mut self, path: &str, json_path: &str) -> String {
        let Ok(text) = std::fs::read_to_string(self.root.join(path)) else {
            return String::new();
        };
        json_value(&text, json_path)
    }
}

#[test]
fn get_field_reads_a_data_asset() {
    let dir = std::env::temp_dir().join(format!("seal-graph-data-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(
        dir.join("data/stats.datatype"),
        "seal_datatype = 1\nname = Stats\nfield hp number\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("data/hero.data"),
        "seal_data = 1\ntype = data/stats.datatype\nvalue hp = 42\n",
    )
    .unwrap();
    std::fs::write(dir.join("tables.csv"), "a,b\n1,9\n").unwrap();
    std::fs::write(dir.join("meta.json"), r#"{"x":3}"#).unwrap();

    let g = Graph::new("data")
        .node(Node::new(0, "get_field").with(
            "defaults",
            seal_sealtext::parse("path = \"data/hero.data\"\nfield = \"hp\"").unwrap(),
        ))
        .node(Node::new(1, "read_csv_cell").with(
            "defaults",
            seal_sealtext::parse("path = \"tables.csv\"\nrow = 1\ncol = 1").unwrap(),
        ))
        .node(Node::new(2, "read_json_value").with(
            "defaults",
            seal_sealtext::parse("path = \"meta.json\"\njson_path = \"x\"").unwrap(),
        ));
    let prog = Program::build(&g).unwrap();
    assert!(matches!(prog.nodes.first().map(|n| &n.op), Some(Op::DataField)));
    let mut host = DataHost { root: dir.clone() };
    let mut i = Interp::new(prog, 0);
    let hp = i.eval_pure(0, &mut host).unwrap();
    assert_eq!(hp.str(), "42");
    let cell = i.eval_pure(1, &mut host).unwrap();
    assert_eq!(cell.str(), "9");
    let json = i.eval_pure(2, &mut host).unwrap();
    assert_eq!(json.str(), "3");
}
