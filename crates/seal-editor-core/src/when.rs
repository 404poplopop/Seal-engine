use crate::error::CoreError;

/// Context keys a `when` expression can read. Values are strings such as `true` or a panel id.
#[derive(Clone, Debug, Default)]
pub struct WhenContext {
    pairs: Vec<(String, String)>,
}

impl WhenContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.pairs.iter_mut().find(|(name, _)| name == &key) {
            slot.1 = value;
        } else {
            self.pairs.push((key, value));
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }
}

/// Empty expressions match. A broken expression does not, so a bad keymap cannot steal a key.
pub fn when_allows(expr: &str, ctx: &WhenContext) -> bool {
    when_matches(expr, ctx).unwrap_or(false)
}

pub fn when_matches(expr: &str, ctx: &WhenContext) -> Result<bool, CoreError> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Ok(true);
    }
    eval_or(expr, ctx)
}

fn eval_or(expr: &str, ctx: &WhenContext) -> Result<bool, CoreError> {
    let parts = split_top(expr, "||")?;
    if parts.len() > 1 {
        for part in parts {
            if eval_and(part, ctx)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    eval_and(expr, ctx)
}

fn eval_and(expr: &str, ctx: &WhenContext) -> Result<bool, CoreError> {
    let parts = split_top(expr, "&&")?;
    if parts.len() > 1 {
        for part in parts {
            if !eval_unary(part, ctx)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    eval_unary(expr, ctx)
}

fn eval_unary(expr: &str, ctx: &WhenContext) -> Result<bool, CoreError> {
    let expr = expr.trim();
    if let Some(rest) = expr.strip_prefix('!') {
        return Ok(!eval_unary(rest, ctx)?);
    }
    if expr.starts_with('(') && expr.ends_with(')') {
        return eval_or(&expr[1..expr.len() - 1], ctx);
    }
    if let Some((left, right, negate)) = comparison(expr)? {
        let actual = ctx.get(left).unwrap_or("");
        let same = actual == right;
        return Ok(if negate { !same } else { same });
    }
    let value = ctx.get(expr).unwrap_or("");
    Ok(value == "true")
}

fn comparison(expr: &str) -> Result<Option<(&str, String, bool)>, CoreError> {
    let (token, negate) = if let Some(index) = find_op(expr, "!=") {
        (index, true)
    } else if let Some(index) = find_op(expr, "==") {
        (index, false)
    } else {
        return Ok(None);
    };
    let op_len = 2;
    let left = expr[..token].trim();
    let right = unquote(expr[token + op_len..].trim())?;
    if left.is_empty() {
        return Err(CoreError::new("when comparison is missing a key"));
    }
    Ok(Some((left, right, negate)))
}

fn unquote(text: &str) -> Result<String, CoreError> {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return Ok(String::new());
    };
    if first == '\'' || first == '"' {
        let rest: String = chars.collect();
        let Some(stripped) = rest.strip_suffix(first) else {
            return Err(CoreError::new("when string is missing its closing quote"));
        };
        return Ok(stripped.to_string());
    }
    Ok(text.to_string())
}

fn find_op(expr: &str, op: &str) -> Option<usize> {
    let mut quote = None;
    let mut index = 0;
    for ch in expr.chars() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if expr[index..].starts_with(op) {
            return Some(index);
        }
        index += ch.len_utf8();
    }
    None
}

fn split_top<'a>(expr: &'a str, op: &str) -> Result<Vec<&'a str>, CoreError> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut depth = 0i32;
    let mut index = 0;
    while index < expr.len() {
        let ch = expr[index..].chars().next().expect("index is on a char boundary");
        let len = ch.len_utf8();
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            index += len;
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 && expr[index..].starts_with(op) => {
                parts.push(expr[start..index].trim());
                index += op.len();
                start = index;
                continue;
            }
            _ => {}
        }
        index += len;
    }
    if quote.is_some() || depth != 0 {
        return Err(CoreError::new("unbalanced when expression"));
    }
    parts.push(expr[start..].trim());
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_read_context_keys() {
        let mut ctx = WhenContext::new();
        ctx.set("hasProject", "true");
        ctx.set("panelFocus", "console");
        ctx.set("textInputFocused", "false");
        assert!(when_matches("hasProject", &ctx).unwrap());
        assert!(when_matches("panelFocus == 'console'", &ctx).unwrap());
        assert!(when_matches("hasProject && panelFocus == 'console'", &ctx).unwrap());
        assert!(when_matches("!textInputFocused", &ctx).unwrap());
        assert!(when_matches("panelFocus == 'outliner' || hasProject", &ctx).unwrap());
        assert!(!when_matches("panelFocus == 'outliner'", &ctx).unwrap());
        assert!(!when_allows("panelFocus ==", &ctx));
    }
}
