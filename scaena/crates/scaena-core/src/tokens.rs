//! Import brand tokens from source: Jetpack Compose `Theme.kt` (`val Name = Color(0xAARRGGBB)`) or CSS
//! custom properties (`--name: #rrggbb`). Pure std, no deps. Feeds the mock-render fallback and lets
//! Scaena frame/caption in the app's own colors.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub name: String,
    pub hex: String, // "#RRGGBB"
}

/// Parse Compose color declarations: `val Amber = Color(0xFFEBA948)` -> Token{Amber, #EBA948}.
/// Accepts 8-digit (ARGB, alpha dropped) or 6-digit hex.
pub fn from_kotlin(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("val ") else { continue };
        let Some(eq) = rest.find('=') else { continue };
        let name = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim();
        // Expect Color(0x........)
        let Some(hx) = after.strip_prefix("Color(0x").or_else(|| after.strip_prefix("Color(0X")) else { continue };
        let digits: String = hx.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if let Some(hex) = normalize_hex(&digits) {
            out.push(Token { name, hex });
        }
    }
    out
}

/// Parse CSS custom properties: `--accent: #eba948;` -> Token{accent, #EBA948}.
pub fn from_css(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for raw in text.split([';', '\n']) {
        let l = raw.trim();
        let Some(rest) = l.strip_prefix("--") else { continue };
        let Some(colon) = rest.find(':') else { continue };
        let name = rest[..colon].trim().to_string();
        let val = rest[colon + 1..].trim().trim_start_matches('#');
        let digits: String = val.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if let Some(hex) = normalize_hex(&digits) {
            out.push(Token { name, hex });
        }
    }
    out
}

/// 8-digit ARGB -> RGB, 6-digit -> as-is, 3-digit -> expanded. None otherwise.
fn normalize_hex(d: &str) -> Option<String> {
    let rgb = match d.len() {
        8 => &d[2..], // drop alpha
        6 => d,
        3 => return Some(format!("#{}{}{}{}{}{}", &d[0..1], &d[0..1], &d[1..2], &d[1..2], &d[2..3], &d[2..3]).to_uppercase()),
        _ => return None,
    };
    Some(format!("#{}", rgb.to_uppercase()))
}

/// Auto-detect by extension (`.kt`/`.kts` -> Kotlin, else CSS-ish).
pub fn import(text: &str, filename: &str) -> Vec<Token> {
    if filename.ends_with(".kt") || filename.ends_with(".kts") {
        from_kotlin(text)
    } else {
        from_css(text)
    }
}

/// Serialize tokens to a small JSON array (no serde dep here).
pub fn to_json(tokens: &[Token]) -> String {
    let items: Vec<String> = tokens.iter().map(|t| format!(r#"{{"name":"{}","hex":"{}"}}"#, t.name, t.hex)).collect();
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_colors() {
        let src = "val Ink = Color(0xFF0E0D10)\nval Amber = Color(0xFFEBA948) // spark\nnot a token\nval Half = Color(0x80FF0000)";
        let t = from_kotlin(src);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0], Token { name: "Ink".into(), hex: "#0E0D10".into() });
        assert_eq!(t[1], Token { name: "Amber".into(), hex: "#EBA948".into() });
        assert_eq!(t[2].hex, "#FF0000"); // alpha dropped
    }

    #[test]
    fn css_vars() {
        let src = ":root{ --accent: #EBA948; --bg:#0e0d10; --x: red; }";
        let t = from_css(src);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], Token { name: "accent".into(), hex: "#EBA948".into() });
        assert_eq!(t[1], Token { name: "bg".into(), hex: "#0E0D10".into() });
    }
}
