//! The block palette, read from the pinned engine's catalog (`oce_api::CATALOG_JSON`).
//! Nothing here is typed by hand.

use std::collections::BTreeMap;

use serde::Deserialize;

const CDL_PREFIX: &str = "CDL.";

#[derive(Debug, thiserror::Error)]
pub enum PaletteError {
    #[error("catalog JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("catalog class {class} has port {port} of unknown kind {kind}")]
    UnknownPortKind {
        class: String,
        port: String,
        kind: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortKind {
    Real,
    Boolean,
    Integer,
}

impl PortKind {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "Real" => Some(PortKind::Real),
            "Boolean" => Some(PortKind::Boolean),
            "Integer" => Some(PortKind::Integer),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PortKind::Real => "Real",
            PortKind::Boolean => "Boolean",
            PortKind::Integer => "Integer",
        }
    }
}

/// What a parameter value must look like. `Unspecified` covers derived parameters the catalog
/// gives no type for; the validator refuses to set those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Real,
    Integer,
    Boolean,
    Enum,
    String,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamSpec {
    pub kind: ParamKind,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSpec {
    pub name: String,
    pub kind: PortKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSpec {
    /// Class path without `CDL.`, e.g. `Logical.TrueDelay`.
    pub class: String,
    /// Ports in declaration order (the order CXF lists them).
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub params: BTreeMap<String, ParamSpec>,
    pub stateful: bool,
    /// False for reserved and width-driven classes, which need port synthesis we do not have yet.
    pub authorable: bool,
}

#[derive(Debug, Clone)]
pub struct Palette {
    classes: BTreeMap<String, ClassSpec>,
}

impl Palette {
    /// The palette of the pinned engine. The same catalog judges every draft.
    pub fn pinned() -> Result<Self, PaletteError> {
        Self::from_catalog_json(oce_api::CATALOG_JSON)
    }

    pub fn from_catalog_json(json: &str) -> Result<Self, PaletteError> {
        let raw: RawCatalog = serde_json::from_str(json)?;
        let mut classes = BTreeMap::new();
        for entry in raw.entries {
            let Some(class) = entry.class_path.strip_prefix(CDL_PREFIX) else {
                continue;
            };
            // Width-driven classes list ports without names; they need synthesis we lack.
            let all_named = entry
                .inputs
                .iter()
                .chain(&entry.outputs)
                .all(|p| p.name.is_some());
            let spec = ClassSpec {
                class: class.to_owned(),
                inputs: ports(class, &entry.inputs)?,
                outputs: ports(class, &entry.outputs)?,
                params: params(&entry),
                stateful: entry.stateful,
                authorable: all_named
                    && !entry.reserved
                    && !entry.width_driven
                    && entry.naming != "width_driven",
            };
            classes.insert(spec.class.clone(), spec);
        }
        Ok(Self { classes })
    }

    pub fn class(&self, class: &str) -> Option<&ClassSpec> {
        self.classes.get(class)
    }

    /// All classes in lexical order.
    pub fn classes(&self) -> impl Iterator<Item = &ClassSpec> {
        self.classes.values()
    }
}

fn ports(class: &str, raw: &[RawPort]) -> Result<Vec<PortSpec>, PaletteError> {
    raw.iter()
        .filter_map(|p| Some((p.name.as_ref()?, p)))
        .map(|(name, p)| {
            let kind = PortKind::parse(&p.kind).ok_or_else(|| PaletteError::UnknownPortKind {
                class: class.to_owned(),
                port: name.clone(),
                kind: p.kind.clone(),
            })?;
            Ok(PortSpec {
                name: name.clone(),
                kind,
            })
        })
        .collect()
}

/// A typed `Required` rule fixes the kind first; otherwise the default's kind decides. This
/// mirrors how the research prototype read the catalog.
fn params(entry: &RawEntry) -> BTreeMap<String, ParamSpec> {
    let mut out: BTreeMap<String, ParamSpec> = BTreeMap::new();
    for rule in &entry.param_rules {
        let (Some(name), "Required") = (&rule.name, rule.rule.as_str()) else {
            continue;
        };
        let kind = match rule.kind.as_ref().map(|k| k.kind.as_str()) {
            Some("Real") => ParamKind::Real,
            Some("Integer") => ParamKind::Integer,
            Some("Boolean") => ParamKind::Boolean,
            Some("Enum") => ParamKind::Enum,
            Some("String") => ParamKind::String,
            _ => ParamKind::Unspecified,
        };
        out.insert(
            name.clone(),
            ParamSpec {
                kind,
                required: true,
            },
        );
    }
    for default in &entry.param_defaults {
        let spec = out.entry(default.name.clone()).or_insert(ParamSpec {
            kind: ParamKind::Unspecified,
            required: false,
        });
        match default.default.kind.as_str() {
            "required" => spec.required = true,
            kind if spec.kind == ParamKind::Unspecified => {
                spec.kind = match kind {
                    "real" => ParamKind::Real,
                    "integer" => ParamKind::Integer,
                    "boolean" => ParamKind::Boolean,
                    "enum" => ParamKind::Enum,
                    _ => ParamKind::Unspecified,
                };
            }
            _ => {}
        }
    }
    out
}

#[derive(Deserialize)]
struct RawCatalog {
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    class_path: String,
    inputs: Vec<RawPort>,
    outputs: Vec<RawPort>,
    naming: String,
    param_defaults: Vec<RawDefault>,
    param_rules: Vec<RawRule>,
    reserved: bool,
    stateful: bool,
    width_driven: bool,
}

#[derive(Deserialize)]
struct RawPort {
    name: Option<String>,
    kind: String,
}

#[derive(Deserialize)]
struct RawDefault {
    name: String,
    default: RawDefaultValue,
}

#[derive(Deserialize)]
struct RawDefaultValue {
    kind: String,
}

#[derive(Deserialize)]
struct RawRule {
    #[serde(default)]
    name: Option<String>,
    rule: String,
    #[serde(default)]
    kind: Option<RawRuleKind>,
}

#[derive(Deserialize)]
struct RawRuleKind {
    kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_palette_has_the_cdl_classes() {
        let palette = Palette::pinned().unwrap();
        assert_eq!(palette.classes().count(), 133);
        let authorable = palette.classes().filter(|c| c.authorable).count();
        // 105 named; the 25 width-driven classes and the 3 positional TimeTables need port synthesis.
        assert_eq!(authorable, 105);
        assert!(!palette.class("Reals.Sources.TimeTable").unwrap().authorable);
    }

    #[test]
    fn true_delay_matches_the_catalog() {
        let palette = Palette::pinned().unwrap();
        let td = palette.class("Logical.TrueDelay").unwrap();
        assert_eq!(
            td.inputs,
            vec![PortSpec {
                name: "u".into(),
                kind: PortKind::Boolean
            }]
        );
        assert_eq!(
            td.outputs,
            vec![PortSpec {
                name: "y".into(),
                kind: PortKind::Boolean
            }]
        );
        assert_eq!(
            td.params.get("delayTime"),
            Some(&ParamSpec {
                kind: ParamKind::Real,
                required: true
            })
        );
        assert_eq!(
            td.params.get("delayOnInit"),
            Some(&ParamSpec {
                kind: ParamKind::Boolean,
                required: false
            })
        );
        assert!(td.stateful && td.authorable);
    }

    #[test]
    fn subtract_keeps_port_order() {
        let palette = Palette::pinned().unwrap();
        let names: Vec<_> = palette
            .class("Reals.Subtract")
            .unwrap()
            .inputs
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["u1", "u2"]);
    }
}
