//! Three-way merge of scene files, for version control.
//!
//! Entities are matched by their file id (their SceneId), components by
//! name, and component fields key by key, so two people editing different
//! entities, different components or different fields of one component
//! merge cleanly. A conflict is reported when both sides changed the same
//! value differently, or one side deleted what the other changed; the merged
//! text then keeps "ours" for that value.

use std::collections::{BTreeMap, BTreeSet};

use seal_sealtext::{parse, SealMap, SealValue};

#[derive(Debug)]
pub struct MergeOutcome {
    pub text: String,
    /// One line per conflict, such as `entity 4, transform.scale: both sides changed it`.
    pub conflicts: Vec<String>,
}

type Entities = BTreeMap<u64, SealValue>;

fn entities(text: &str, which: &str) -> Result<(u64, Entities), String> {
    let scene = parse(text).map_err(|e| format!("{which}: {e}"))?;
    let version = match scene.get("seal_scene") {
        Some(SealValue::Number(n)) => n.parse().map_err(|_| format!("{which}: bad seal_scene"))?,
        _ => return Err(format!("{which}: not a Seal scene")),
    };
    let mut out = BTreeMap::new();
    if let Some(SealValue::Array(rows)) = scene.get("entities") {
        for row in rows {
            let id = match row.get("id") {
                Some(SealValue::Number(n)) => {
                    n.parse().map_err(|_| format!("{which}: bad entity id {n}"))?
                }
                _ => return Err(format!("{which}: entity without id")),
            };
            let components =
                row.get("components").cloned().unwrap_or(SealValue::Map(SealMap::new(vec![])));
            if out.insert(id, components).is_some() {
                return Err(format!("{which}: entity id {id} appears twice"));
            }
        }
    }
    Ok((version, out))
}

fn keys(value: Option<&SealValue>) -> Vec<&str> {
    value
        .and_then(SealValue::as_map)
        .map_or(vec![], |m| m.iter().map(|(k, _)| k.as_str()).collect())
}

/// Merge one value. `None` means absent on that side.
fn merge_value(
    base: Option<&SealValue>,
    ours: Option<&SealValue>,
    theirs: Option<&SealValue>,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Option<SealValue> {
    if ours == theirs || theirs == base {
        return ours.cloned();
    }
    if ours == base {
        return theirs.cloned();
    }
    let all_maps = [ours, theirs].iter().all(|v| v.is_some_and(|v| v.as_map().is_some()))
        && base.is_none_or(|b| b.as_map().is_some());
    if all_maps {
        let mut order: Vec<&str> = keys(ours);
        for key in keys(theirs).into_iter().chain(keys(base)) {
            if !order.contains(&key) {
                order.push(key);
            }
        }
        let mut entries = Vec::new();
        for key in order {
            let get = |v: Option<&SealValue>| v.and_then(|v| v.get(key)).cloned();
            let (b, o, t) = (get(base), get(ours), get(theirs));
            let sub = if path.is_empty() { key.to_string() } else { format!("{path}.{key}") };
            if let Some(value) = merge_value(b.as_ref(), o.as_ref(), t.as_ref(), &sub, conflicts) {
                entries.push((key.to_string(), value));
            }
        }
        return Some(SealValue::Map(SealMap::new(entries)));
    }
    let what = match (ours, theirs) {
        (None, _) => "we deleted it and they changed it",
        (_, None) => "they deleted it and we changed it",
        _ if base.is_none() => "both sides added it differently",
        _ => "both sides changed it",
    };
    conflicts.push(format!("{path}: {what}"));
    ours.or(theirs).cloned()
}

/// Merge `ours` and `theirs`, which both started from `base`.
pub fn merge_scenes(
    base_text: &str,
    ours_text: &str,
    theirs_text: &str,
) -> Result<MergeOutcome, String> {
    let (vb, base) = entities(base_text, "base")?;
    let (vo, ours) = entities(ours_text, "ours")?;
    let (vt, theirs) = entities(theirs_text, "theirs")?;
    let ids: BTreeSet<u64> = base.keys().chain(ours.keys()).chain(theirs.keys()).copied().collect();
    let mut conflicts = Vec::new();
    let mut rows = Vec::new();
    for id in ids {
        let mut local = Vec::new();
        let merged = merge_value(base.get(&id), ours.get(&id), theirs.get(&id), "", &mut local);
        conflicts.extend(local.into_iter().map(|c| {
            if c.starts_with(':') {
                format!("entity {id}{c}")
            } else {
                format!("entity {id}, {c}")
            }
        }));
        if let Some(components) = merged {
            rows.push(SealValue::Map(SealMap::new(vec![
                ("id".into(), SealValue::Number(id.to_string())),
                ("components".into(), components),
            ])));
        }
    }
    let version = vb.max(vo).max(vt);
    let mut header = vec![("seal_scene".into(), SealValue::Number(version.to_string()))];
    // Component versions and scene-wide resources merge key by key like components.
    let (base, ours, theirs) =
        (parse(base_text).ok(), parse(ours_text).ok(), parse(theirs_text).ok());
    for key in ["versions", "resources"] {
        let mut local = Vec::new();
        let merged = merge_value(
            base.as_ref().and_then(|s| s.get(key)),
            ours.as_ref().and_then(|s| s.get(key)),
            theirs.as_ref().and_then(|s| s.get(key)),
            key,
            &mut local,
        );
        conflicts.extend(local);
        if let Some(value) = merged {
            header.push((key.into(), value));
        }
    }
    header.push(("entities".into(), SealValue::Array(rows)));
    let scene = SealValue::Map(SealMap::new(header));
    Ok(MergeOutcome { text: scene.to_text(), conflicts })
}
