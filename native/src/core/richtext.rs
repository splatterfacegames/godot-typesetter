//! Rich-text tag tokenizer. Recognises the tag syntax documented for TextMesh
//! Pro (`<b>`, `<color=#RRGGBB>`, `<size=150%>`, `<link="id">`, `<sprite name="x">`,
//! closing tags `</b>`, and `<noparse>` regions) and leaves anything else as
//! literal text. Interpretation of each tag happens in the layout engine.

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    /// Lower-cased tag name without '/' (e.g. "color", "size", "#ff0000" for the
    /// bare colour shorthand).
    pub name: String,
    pub closing: bool,
    /// Value after '=' on the tag name (quotes removed), if any.
    pub value: Option<String>,
    /// Other attributes, lower-cased keys: `<sprite name="x" tint=1>`.
    pub attrs: Vec<(String, String)>,
    /// Source span (start, end exclusive) in chars.
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// A character and its index in the source string (chars).
    Char(char, usize),
    Tag(Tag),
}

const KNOWN: &[&str] = &[
    "b", "i", "u", "s", "color", "alpha", "size", "font", "font-weight", "material", "sprite", "link", "a", "mark",
    "sup", "sub", "cspace", "mspace", "voffset", "indent", "line-height", "line-indent", "margin", "margin-left",
    "margin-right", "align", "allcaps", "smallcaps", "lowercase", "uppercase", "nobr", "noparse", "page", "pos", "rotate",
    "space", "width", "gradient", "style", "br", "zwsp", "nbsp", "shy", "zwj", "strikethrough", "underline", "fade",
    "action",
];

fn parse_tag(body: &str, start: usize, end: usize) -> Option<Tag> {
    let b = body.trim();
    if b.is_empty() {
        return None;
    }
    let (closing, b) = if let Some(rest) = b.strip_prefix('/') { (true, rest.trim_start()) } else { (false, b) };
    // Colour shorthand: <#RRGGBB> / <#RGB> / <#RRGGBBAA>.
    if let Some(hex) = b.strip_prefix('#') {
        if !closing && matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(Tag { name: "color".into(), closing: false, value: Some(format!("#{hex}")), attrs: vec![], span: (start, end) });
        }
        return None;
    }
    // Split name[=value] from attributes, honouring quotes.
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in b.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                cur.push(c);
            }
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    parts.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let unquote = |s: &str| -> String {
        let s = s.trim();
        if s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''))) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    };
    let first = parts.first()?.clone();
    let (name, value) = match first.split_once('=') {
        Some((n, v)) => (n.to_ascii_lowercase(), Some(unquote(v))),
        None => (first.to_ascii_lowercase(), None),
    };
    if !KNOWN.contains(&name.as_str()) {
        return None;
    }
    let mut attrs = Vec::new();
    for p in &parts[1..] {
        if let Some((k, v)) = p.split_once('=') {
            attrs.push((k.to_ascii_lowercase(), unquote(v)));
        } else {
            attrs.push((p.to_ascii_lowercase(), String::new()));
        }
    }
    Some(Tag { name, closing, value, attrs, span: (start, end) })
}

/// Tokenizes `text`. With `rich_text` false every character is literal.
pub fn tokenize(text: &str, rich_text: bool) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    let mut noparse = false;
    while i < chars.len() {
        let c = chars[i];
        if rich_text && c == '<' {
            // Find the closing '>' (tags never span '<').
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' && chars[j] != '<' && j - i < 512 {
                j += 1;
            }
            if j < chars.len() && chars[j] == '>' {
                let body: String = chars[i + 1..j].iter().collect();
                if let Some(tag) = parse_tag(&body, i, j + 1) {
                    if noparse {
                        if tag.closing && tag.name == "noparse" {
                            noparse = false;
                            i = j + 1;
                            continue;
                        }
                    } else if tag.name == "noparse" && !tag.closing {
                        noparse = true;
                        i = j + 1;
                        continue;
                    } else {
                        out.push(Token::Tag(tag));
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        // Escapes TMP documents for literal control characters in text fields.
        if c == '\\' && i + 1 < chars.len() && !noparse {
            let mapped = match chars[i + 1] {
                'n' => Some('\n'),
                't' => Some('\t'),
                'r' => Some('\r'),
                _ => None,
            };
            if let Some(m) = mapped {
                out.push(Token::Char(m, i));
                i += 2;
                continue;
            }
        }
        out.push(Token::Char(c, i));
        i += 1;
    }
    out
}

/// Parses a colour value: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `"red"` etc.
pub fn parse_color(v: &str) -> Option<[f32; 4]> {
    let v = v.trim().trim_matches('"');
    if let Some(hex) = v.strip_prefix('#') {
        let h = |s: &str| u8::from_str_radix(s, 16).ok().map(|x| x as f32 / 255.0);
        let d = |c: char| c.to_digit(16).map(|x| (x * 17) as f32 / 255.0);
        let cs: Vec<char> = hex.chars().collect();
        return match cs.len() {
            3 => Some([d(cs[0])?, d(cs[1])?, d(cs[2])?, 1.0]),
            4 => Some([d(cs[0])?, d(cs[1])?, d(cs[2])?, d(cs[3])?]),
            6 => Some([h(&hex[0..2])?, h(&hex[2..4])?, h(&hex[4..6])?, 1.0]),
            8 => Some([h(&hex[0..2])?, h(&hex[2..4])?, h(&hex[4..6])?, h(&hex[6..8])?]),
            _ => None,
        };
    }
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => [0.0, 0.0, 0.0, 1.0],
        "blue" => [0.0, 0.0, 1.0, 1.0],
        "green" => [0.0, 1.0, 0.0, 1.0],
        "orange" => [1.0, 128.0 / 255.0, 0.0, 1.0],
        "purple" => [160.0 / 255.0, 32.0 / 255.0, 240.0 / 255.0, 1.0],
        "red" => [1.0, 0.0, 0.0, 1.0],
        "white" => [1.0, 1.0, 1.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0, 1.0],
        _ => return None,
    })
}

/// A length value with its unit as written in a tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// Plain number: pixels (font units scaled to the current point size).
    Pixels(f32),
    /// `em` units: multiples of the current font size.
    Em(f32),
    /// Percentage (meaning depends on the tag: of font size, of line width…).
    Percent(f32),
}

pub fn parse_length(v: &str) -> Option<Length> {
    let v = v.trim().trim_matches('"');
    if let Some(n) = v.strip_suffix('%') {
        return n.trim().parse().ok().map(Length::Percent);
    }
    if let Some(n) = v.strip_suffix("em") {
        return n.trim().parse().ok().map(Length::Em);
    }
    let n = v.strip_suffix("px").unwrap_or(v);
    n.trim().parse().ok().map(Length::Pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &str) -> Vec<String> {
        tokenize(t, true)
            .into_iter()
            .filter_map(|x| match x {
                Token::Tag(t) => Some(format!("{}{}{}", if t.closing { "/" } else { "" }, t.name, t.value.map(|v| format!("={v}")).unwrap_or_default())),
                _ => None,
            })
            .collect()
    }

    fn text(t: &str) -> String {
        tokenize(t, true).into_iter().filter_map(|x| if let Token::Char(c, _) = x { Some(c) } else { None }).collect()
    }

    #[test]
    fn basic_tags() {
        assert_eq!(tags("<b>x</b><color=#ff0000>y</color><#00f>z"), vec!["b", "/b", "color=#ff0000", "/color", "color=#00f"]);
        assert_eq!(text("<b>x</b> <size=50%>y"), "x y");
        assert_eq!(tags("<link=\"my id\">a</link>"), vec!["link=my id", "/link"]);
    }

    #[test]
    fn unknown_and_broken_tags_are_literal() {
        assert_eq!(text("a <foo> b < c"), "a <foo> b < c");
        assert_eq!(text("1 < 2 > 0"), "1 < 2 > 0");
    }

    #[test]
    fn noparse_region() {
        assert_eq!(text("<noparse><b>bold</b></noparse>!"), "<b>bold</b>!");
    }

    #[test]
    fn attributes() {
        let t = tokenize("<sprite name=\"smile\" tint=1>", true);
        let Token::Tag(tag) = &t[0] else { panic!() };
        assert_eq!(tag.name, "sprite");
        assert_eq!(tag.attrs, vec![("name".to_string(), "smile".to_string()), ("tint".to_string(), "1".to_string())]);
    }

    #[test]
    fn colours_and_lengths() {
        assert_eq!(parse_color("#FF000080"), Some([1.0, 0.0, 0.0, 128.0 / 255.0]));
        assert_eq!(parse_color("red"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_length("150%"), Some(Length::Percent(150.0)));
        assert_eq!(parse_length("1.5em"), Some(Length::Em(1.5)));
        assert_eq!(parse_length("12"), Some(Length::Pixels(12.0)));
    }
}
