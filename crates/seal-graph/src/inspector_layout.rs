//! Custom inspector layout stored on a graph script (field order, sections, hiding, ranges, help).

use seal_sealtext::SealValue;

/// One section heading in the custom inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorSection {
    pub name: String,
    pub order: i32,
}

/// Rules for one entity field row in the inspector.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct InspectorFieldRule {
    pub field: String,
    pub section: Option<String>,
    pub order: i32,
    pub hidden: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub help: Option<String>,
}

/// Layout declaration for objects using this graph script.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct InspectorLayout {
    pub sections: Vec<InspectorSection>,
    pub fields: Vec<InspectorFieldRule>,
}

impl InspectorLayout {
    pub fn rule_for(&self, field: &str) -> Option<&InspectorFieldRule> {
        self.fields.iter().find(|rule| rule.field == field)
    }

    pub fn is_hidden(&self, field: &str) -> bool {
        self.rule_for(field).is_some_and(|r| r.hidden)
    }

    pub fn help_for(&self, field: &str) -> Option<&str> {
        self.rule_for(field).and_then(|r| r.help.as_deref())
    }

    pub fn range_for(&self, field: &str) -> (Option<f32>, Option<f32>) {
        self.rule_for(field)
            .map(|r| {
                (
                    r.min.filter(|v| v.is_finite()).map(|v| v as f32),
                    r.max.filter(|v| v.is_finite()).map(|v| v as f32),
                )
            })
            .unwrap_or((None, None))
    }

    pub fn to_seal(&self) -> SealValue {
        let sections = self
            .sections
            .iter()
            .map(|s| {
                SealValue::Map(seal_sealtext::SealMap::new(vec![
                    ("name".into(), SealValue::String(s.name.clone())),
                    ("order".into(), SealValue::Number(s.order.to_string())),
                ]))
            })
            .collect();
        let fields = self
            .fields
            .iter()
            .map(|f| {
                let mut pairs = vec![
                    ("field".into(), SealValue::String(f.field.clone())),
                    ("order".into(), SealValue::Number(f.order.to_string())),
                    (
                        "hidden".into(),
                        SealValue::Bool(f.hidden),
                    ),
                ];
                if let Some(section) = &f.section {
                    pairs.push(("section".into(), SealValue::String(section.clone())));
                }
                if let Some(min) = f.min.filter(|v| v.is_finite()) {
                    pairs.push(("min".into(), SealValue::Number(min.to_string())));
                }
                if let Some(max) = f.max.filter(|v| v.is_finite()) {
                    pairs.push(("max".into(), SealValue::Number(max.to_string())));
                }
                if let Some(help) = &f.help {
                    if !help.is_empty() {
                        pairs.push(("help".into(), SealValue::String(help.clone())));
                    }
                }
                SealValue::Map(seal_sealtext::SealMap::new(pairs))
            })
            .collect();
        SealValue::Map(seal_sealtext::SealMap::new(vec![
            ("sections".into(), SealValue::Array(sections)),
            ("fields".into(), SealValue::Array(fields)),
        ]))
    }

    pub fn from_seal(value: &SealValue) -> Result<Self, String> {
        if value.as_map().is_none() {
            return Err("inspector layout must be a map".into());
        }
        let list = |key: &str| -> Vec<SealValue> {
            value
                .get(key)
                .and_then(|v| match v {
                    SealValue::Array(a) => Some(a.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        };
        let mut sections = Vec::new();
        for (i, item) in list("sections").into_iter().enumerate() {
            let name = item
                .get("name")
                .and_then(|v| match v {
                    SealValue::String(s) => Some(s.clone()),
                    _ => None,
                })
                .ok_or_else(|| format!("section {i} has no name"))?;
            let order = item
                .get("order")
                .and_then(|v| match v {
                    SealValue::Number(n) => n.parse().ok(),
                    _ => None,
                })
                .unwrap_or(i as i32);
            sections.push(InspectorSection { name, order });
        }
        let mut fields = Vec::new();
        for (i, item) in list("fields").into_iter().enumerate() {
            let field = item
                .get("field")
                .and_then(|v| match v {
                    SealValue::String(s) => Some(s.clone()),
                    _ => None,
                })
                .ok_or_else(|| format!("field rule {i} has no field name"))?;
            let order = item
                .get("order")
                .and_then(|v| match v {
                    SealValue::Number(n) => n.parse().ok(),
                    _ => None,
                })
                .unwrap_or(i as i32);
            let hidden = matches!(item.get("hidden"), Some(SealValue::Bool(true)));
            let section = item.get("section").and_then(|v| match v {
                SealValue::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            });
            let parse_f = |key: &str| -> Option<f64> {
                item.get(key).and_then(|v| match v {
                    SealValue::Number(n) => n.parse().ok(),
                    SealValue::String(n) => n.parse().ok(),
                    _ => None,
                })
            };
            let help = item.get("help").and_then(|v| match v {
                SealValue::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            });
            fields.push(InspectorFieldRule {
                field,
                section,
                order,
                hidden,
                min: parse_f("min"),
                max: parse_f("max"),
                help,
            });
        }
        Ok(Self { sections, fields })
    }
}

/// One row after layout rules are applied (for tests and the inspector panel).
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorRowPlan {
    pub field: String,
    pub hidden: bool,
    pub order: i32,
    pub section: Option<String>,
    pub help: Option<String>,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

/// Merge default field names with layout rules: order, hiding, ranges, help.
pub fn plan_rows(default_fields: &[&str], layout: Option<&InspectorLayout>) -> Vec<InspectorRowPlan> {
    let Some(layout) = layout else {
        return default_fields
            .iter()
            .enumerate()
            .map(|(i, field)| InspectorRowPlan {
                field: (*field).to_string(),
                hidden: false,
                order: i32::try_from(i).unwrap_or(i32::MAX),
                section: None,
                help: None,
                min: None,
                max: None,
            })
            .collect();
    };
    let mut rows: Vec<InspectorRowPlan> = default_fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let rule = layout.rule_for(field);
            let (min, max) = layout.range_for(field);
            InspectorRowPlan {
                field: (*field).to_string(),
                hidden: rule.is_some_and(|r| r.hidden),
                order: rule.map(|r| r.order).unwrap_or_else(|| i32::try_from(i).unwrap_or(i32::MAX)),
                section: rule.and_then(|r| r.section.clone()),
                help: rule.and_then(|r| r.help.clone()),
                min: min.or(None),
                max: max.or(None),
            }
        })
        .collect();
    rows.sort_by_key(|row| (row.order, row.field.clone()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_round_trip_and_plan() {
        let mut layout = InspectorLayout::default();
        layout.sections.push(InspectorSection {
            name: "Stats".into(),
            order: 0,
        });
        layout.fields.push(InspectorFieldRule {
            field: "walk_speed".into(),
            section: Some("Stats".into()),
            order: 1,
            hidden: false,
            min: Some(0.0),
            max: Some(20.0),
            help: Some("Units per second".into()),
        });
        layout.fields.push(InspectorFieldRule {
            field: "bounce".into(),
            section: None,
            order: 99,
            hidden: true,
            min: None,
            max: None,
            help: None,
        });
        let seal = layout.to_seal();
        let back = InspectorLayout::from_seal(&seal).expect("parse");
        assert_eq!(back.fields.len(), 2);
        let fields = &["x", "walk_speed", "bounce", "y"];
        let plan = plan_rows(fields, Some(&layout));
        assert!(plan.iter().find(|r| r.field == "bounce").unwrap().hidden);
        assert_eq!(
            plan.iter().find(|r| r.field == "walk_speed").unwrap().min,
            Some(0.0)
        );
        assert!(plan[0].field == "walk_speed" || plan[0].order <= plan[1].order);
    }

    #[test]
    fn damaged_layout_is_refused() {
        assert!(InspectorLayout::from_seal(&SealValue::String("nope".into())).is_err());
    }

    #[test]
    fn objects_without_layout_unchanged() {
        let fields = &["a", "b", "c"];
        let plan = plan_rows(fields, None);
        assert_eq!(plan.len(), 3);
        assert!(!plan.iter().any(|r| r.hidden));
        assert_eq!(plan[0].field, "a");
    }
}