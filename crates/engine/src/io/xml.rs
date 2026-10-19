//! Minimal XML DOM built on quick-xml, used by the xlsx reader.

use quick_xml::events::Event;
use quick_xml::Reader;

/// An XML element with local (prefix-free) names.
#[derive(Debug, Clone, Default)]
pub struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<El>,
    /// Concatenated direct text content.
    pub text: String,
}

impl El {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    pub fn attr_f64(&self, k: &str) -> Option<f64> {
        self.attr(k).and_then(|v| v.trim().parse().ok())
    }
    pub fn attr_u32(&self, k: &str) -> Option<u32> {
        self.attr(k).and_then(|v| v.trim().parse::<f64>().ok()).map(|f| f.max(0.0) as u32)
    }
    /// Boolean attribute ("1"/"true"); `default` when absent.
    pub fn attr_bool(&self, k: &str, default: bool) -> bool {
        match self.attr(k) {
            Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
            None => default,
        }
    }
    pub fn child(&self, name: &str) -> Option<&El> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// First descendant (depth-first) with the given name.
    pub fn find(&self, name: &str) -> Option<&El> {
        for c in &self.children {
            if c.name == name {
                return Some(c);
            }
            if let Some(f) = c.find(name) {
                return Some(f);
            }
        }
        None
    }
    /// All descendants with the given name.
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a El>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.find_all(name, out);
        }
    }
    /// Concatenated text of all `<t>` descendants, skipping phonetic runs (`rPh`).
    pub fn rich_text(&self) -> String {
        let mut s = String::new();
        fn walk(e: &El, s: &mut String) {
            for c in &e.children {
                if c.name == "rPh" || c.name == "phoneticPr" {
                    continue;
                }
                if c.name == "t" {
                    s.push_str(&c.text);
                } else {
                    walk(c, s);
                }
            }
        }
        if self.name == "t" {
            return self.text.clone();
        }
        walk(self, &mut s);
        s
    }
}

fn local(name: &str) -> &str {
    match name.rfind(':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Resolve XML entities and character references.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find(';') {
            Some(j) if j <= 10 => {
                let ent = &after[..j];
                if let Some(c) = resolve_entity(ent) {
                    out.push(c);
                } else {
                    out.push('&');
                    out.push_str(ent);
                    out.push(';');
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn resolve_entity(ent: &str) -> Option<char> {
    Some(match ent {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let n = ent.strip_prefix('#')?;
            let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                u32::from_str_radix(h, 16).ok()?
            } else {
                n.parse().ok()?
            };
            char::from_u32(v)?
        }
    })
}

/// Decode Excel's `_xHHHH_` escapes used for control characters in strings.
pub fn decode_excel_escapes(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if i + 7 <= s.len() && &s[i..i + 2] == "_x" && b[i + 6] == b'_' {
            if let Ok(v) = u32::from_str_radix(&s[i + 2..i + 6], 16) {
                if let Some(c) = char::from_u32(v) {
                    out.push(c);
                    i += 7;
                    continue;
                }
            }
        }
        let c = s[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Parse an XML document into its root element.
pub fn parse(src: &str) -> Result<El, String> {
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<El> = vec![El { name: "#document".into(), ..Default::default() }];
    loop {
        let ev = reader.read_event().map_err(|e| format!("XML error: {}", e))?;
        match ev {
            Event::Start(e) => {
                let mut el = El { name: local(e.name().as_ref()).to_string(), ..Default::default() };
                for a in e.attributes().with_checks(false).flatten() {
                    el.attrs.push((local(a.key.as_ref()).to_string(), unescape(&a.value)));
                }
                stack.push(el);
            }
            Event::Empty(e) => {
                let mut el = El { name: local(e.name().as_ref()).to_string(), ..Default::default() };
                for a in e.attributes().with_checks(false).flatten() {
                    el.attrs.push((local(a.key.as_ref()).to_string(), unescape(&a.value)));
                }
                stack.last_mut().unwrap().children.push(el);
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let el = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(el);
                }
            }
            Event::Text(t) => {
                let raw = t.xml10_content();
                stack.last_mut().unwrap().text.push_str(&unescape(&raw));
            }
            Event::CData(t) => {
                let raw = t.into_inner();
                stack.last_mut().unwrap().text.push_str(raw.as_ref());
            }
            Event::GeneralRef(r) => {
                let name = r.into_inner();
                if let Some(c) = resolve_entity(&name) {
                    stack.last_mut().unwrap().text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(el);
    }
    let doc = stack.pop().unwrap();
    doc.children.into_iter().next().ok_or_else(|| "empty XML document".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic() {
        let e = parse(r#"<?xml version="1.0"?><a:root xmlns:a="x" k="v&amp;w"><b>t&lt;1&#65;</b><c/></a:root>"#).unwrap();
        assert_eq!(e.name, "root");
        assert_eq!(e.attr("k"), Some("v&w"));
        assert_eq!(e.child("b").unwrap().text, "t<1A");
        assert!(e.child("c").is_some());
        assert_eq!(decode_excel_escapes("a_x000D_b"), "a\rb");
    }
}
