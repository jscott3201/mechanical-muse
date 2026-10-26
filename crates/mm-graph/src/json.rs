//! A minimal ordered JSON tree and a writer that matches Python's `json.dumps(obj, indent=2)`
//! (ASCII-only output). Object keys keep insertion order, so emitted bytes never depend on map
//! iteration. CXF here only needs strings, arrays and objects.

pub enum Json {
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(&'static str, Json)>),
}

impl Json {
    pub fn str(s: impl Into<String>) -> Self {
        Json::Str(s.into())
    }

    /// Python's `json.dumps(value, indent=2)` followed by a newline.
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Json::Str(s) => write_str(out, s),
            Json::Arr(items) if items.is_empty() => out.push_str("[]"),
            Json::Obj(fields) if fields.is_empty() => out.push_str("{}"),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Json::Obj(fields) => {
                out.push('{');
                for (i, (key, value)) in fields.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    write_str(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn matches_python_indent_2() {
        let doc = Json::Obj(vec![
            ("a", Json::str("x")),
            (
                "b",
                Json::Arr(vec![Json::str("1"), Json::Obj(vec![("c", Json::str("2"))])]),
            ),
            ("d", Json::Arr(vec![])),
        ]);
        let want = "{\n  \"a\": \"x\",\n  \"b\": [\n    \"1\",\n    {\n      \"c\": \"2\"\n    }\n  ],\n  \"d\": []\n}\n";
        assert_eq!(doc.to_pretty(), want);
    }

    #[test]
    fn escapes_like_ensure_ascii() {
        let doc = Json::str("q\"\\\n\u{1}°😀");
        assert_eq!(
            doc.to_pretty(),
            "\"q\\\"\\\\\\n\\u0001\\u00b0\\ud83d\\ude00\"\n"
        );
    }
}
