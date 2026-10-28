//! The block palette, read from the pinned engine's catalog (`oce_api::CATALOG_JSON`).
//! Nothing here is typed by hand.

use std::collections::BTreeMap;

use serde::Deserialize;

const CDL_PREFIX: &str = "CDL.";

/// Width-driven classes we expand ourselves: `nin` inputs named `u1`..`u<nin>`, all of the output's
/// kind, and one output `y`. The engine binds these ports by position, so the names are ours.
/// Others stay out until a sequence needs them (`MultiSum` also has one gain per input).
const WIDTH_CLASSES: &[&str] = &["Reals.MultiMax", "Reals.MultiMin"];

/// The parameter that sets a width class's input count.
pub const WIDTH_PARAM: &str = "nin";

/// The widest block we offer. Enough for a day of hourly values; keeps drafts and prompts small.
pub const MAX_WIDTH: u32 = 32;

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
    #[error("catalog class {0} no longer has the shape we expand (one output, only nin)")]
    UnexpectedWidthClass(String),
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
    /// False for reserved classes and for width-driven ones outside [`WIDTH_CLASSES`].
    pub authorable: bool,
    /// For a width class, the kind of its `u1`..`u<nin>` inputs; `inputs` is then empty and
    /// [`ClassSpec::input_ports`] builds them per block.
    pub width: Option<PortKind>,
}

impl ClassSpec {
    /// A block's input ports. A width class gets `u1`..`u<nin>` from its own `nin`; none when
    /// `nin` is missing or out of range (the validator reports that).
    pub fn input_ports(&self, params: &[crate::graph::Param]) -> Vec<PortSpec> {
        let Some(kind) = self.width else {
            return self.inputs.clone();
        };
        let n = params
            .iter()
            .find(|p| p.name == WIDTH_PARAM)
            .and_then(|p| width_value(&p.value))
            .unwrap_or(0);
        (1..=n)
            .map(|i| PortSpec {
                name: format!("u{i}"),
                kind,
            })
            .collect()
    }
}

/// `nin` as an input count, if it is a whole number in `1..=MAX_WIDTH`.
pub fn width_value(value: &crate::graph::ParamValue) -> Option<u32> {
    let crate::graph::ParamValue::Number(x) = value else {
        return None;
    };
    if x.fract() != 0.0 || *x < 1.0 || *x > f64::from(MAX_WIDTH) {
        return None;
    }
    // Checked above: a whole number in 1..=MAX_WIDTH.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(*x as u32)
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
            let spec = if entry.width_driven && WIDTH_CLASSES.contains(&class) {
                width_class(class, &entry)?
            } else {
                ClassSpec {
                    class: class.to_owned(),
                    inputs: ports(class, &entry.inputs)?,
                    outputs: ports(class, &entry.outputs)?,
                    params: params(&entry),
                    stateful: entry.stateful,
                    authorable: all_named
                        && !needs_unsupported_param(&params(&entry))
                        && !entry.reserved
                        && !entry.width_driven
                        && entry.naming != "width_driven",
                    width: None,
                }
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

/// A width class: one output named `y`, inputs built per block, and `nin` required (the catalog's
/// default of 0 inputs is not a useful block).
fn width_class(class: &str, entry: &RawEntry) -> Result<ClassSpec, PaletteError> {
    let [out] = entry.outputs.as_slice() else {
        return Err(PaletteError::UnexpectedWidthClass(class.to_owned()));
    };
    let kind = PortKind::parse(&out.kind).ok_or_else(|| PaletteError::UnknownPortKind {
        class: class.to_owned(),
        port: "y".to_owned(),
        kind: out.kind.clone(),
    })?;
    let mut params = params(entry);
    if params.len() != 1 || !params.contains_key(WIDTH_PARAM) || !entry.inputs.is_empty() {
        return Err(PaletteError::UnexpectedWidthClass(class.to_owned()));
    }
    params.insert(
        WIDTH_PARAM.to_owned(),
        ParamSpec {
            kind: ParamKind::Integer,
            required: true,
        },
    );
    Ok(ClassSpec {
        class: class.to_owned(),
        inputs: Vec::new(),
        outputs: vec![PortSpec {
            name: "y".to_owned(),
            kind,
        }],
        params,
        stateful: entry.stateful,
        authorable: !entry.reserved,
        width: Some(kind),
    })
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
                    // A formula default (e.g. falseHoldDuration = trueHoldDuration). Every
                    // derived default on an authorable class is a Real.
                    "derived" => ParamKind::Real,
                    _ => ParamKind::Unspecified,
                };
            }
            _ => {}
        }
    }
    out
}

/// A required enum or string parameter (e.g. `CalendarTime.zerTim`) cannot be set yet, so the
/// class could never validate.
fn needs_unsupported_param(params: &BTreeMap<String, ParamSpec>) -> bool {
    params.values().any(|p| {
        p.required
            && matches!(
                p.kind,
                ParamKind::Enum | ParamKind::String | ParamKind::Unspecified
            )
    })
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
        // 105 named classes, minus CalendarTime (enum zerTim) and Assert (string message), which
        // need parameter kinds we cannot set yet, plus the two width classes we expand (MultiMax,
        // MultiMin). Other width-driven classes and positional TimeTables need port synthesis.
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
    fn multi_max_expands_from_nin() {
        use crate::graph::{Param, ParamValue};
        let palette = Palette::pinned().unwrap();
        let mm = palette.class("Reals.MultiMax").unwrap();
        assert!(mm.authorable);
        assert_eq!(mm.width, Some(PortKind::Real));
        assert_eq!(mm.outputs[0].name, "y");
        assert_eq!(
            mm.params.get(WIDTH_PARAM),
            Some(&ParamSpec {
                kind: ParamKind::Integer,
                required: true
            })
        );
        let nin = |x: f64| {
            vec![Param {
                name: WIDTH_PARAM.into(),
                value: ParamValue::Number(x),
            }]
        };
        let names: Vec<_> = mm
            .input_ports(&nin(3.0))
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["u1", "u2", "u3"]);
        for bad in [0.0, 2.5, 33.0, -1.0] {
            assert!(mm.input_ports(&nin(bad)).is_empty(), "nin {bad}");
        }
        assert!(mm.input_ports(&[]).is_empty());
        assert!(!palette.class("Reals.MultiSum").unwrap().authorable);
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
