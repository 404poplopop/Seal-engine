//! 2D camera cull mask: hide sprites on sorting layers that are turned off.

/// Sorting layer index 0..31 is included when its bit is set in `mask`.
pub fn layer_visible(mask: u32, layer: i32) -> bool {
    if layer < 0 || layer >= 32 {
        return true;
    }
    mask & (1u32 << layer) != 0
}

/// Filter draw candidates by sorting layer. `entries` are `(id, layer)` pairs.
pub fn filter_draw_list<'a>(entries: &'a [(String, i32)], mask: u32) -> Vec<&'a str> {
    entries
        .iter()
        .filter(|(_, layer)| layer_visible(mask, *layer))
        .map(|(id, _)| id.as_str())
        .collect()
}

pub fn parse_cull_mask(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cull_mask_filters_the_draw_list() {
        let items = vec![("a".into(), 0), ("b".into(), 1), ("c".into(), 2)];
        let mask = 0b101;
        let visible = filter_draw_list(&items, mask);
        assert_eq!(visible, vec!["a", "c"]);
        assert!(layer_visible(u32::MAX, 5));
        assert!(!layer_visible(1, 1));
        assert_eq!(parse_cull_mask("bad"), u32::MAX);
    }
}
