//! Serializable mirrors of engine values, diagnostics and errors. The engine's own types have no
//! `Serialize` (and `Value` no `PartialEq`), so these are the only shapes that leave `mm-bridge`.

use oce_api::{
    DiagnosticRecord, DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, OcError, Value,
};
use serde::{Serialize, Serializer};

/// An engine value on the wire. Non-finite reals serialize as the strings `"NaN"`, `"Infinity"`
/// and `"-Infinity"`, because JSON numbers cannot carry them.
#[derive(Debug, Clone, PartialEq)]
pub enum WireValue {
    Real(f64),
    Integer(i64),
    Boolean(bool),
    Enum { class: u32, ordinal: u32 },
    Text(String),
}

impl WireValue {
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Real(x) => WireValue::Real(*x),
            Value::Integer(n) => WireValue::Integer(*n),
            Value::Boolean(b) => WireValue::Boolean(*b),
            Value::String(s) => WireValue::Text(s.to_string()),
            Value::Enum { class, ordinal } => WireValue::Enum {
                class: class.0,
                ordinal: *ordinal,
            },
        }
    }
}

impl Serialize for WireValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            WireValue::Real(x) if x.is_nan() => serializer.serialize_str("NaN"),
            WireValue::Real(x) if x.is_infinite() => {
                serializer.serialize_str(if *x > 0.0 { "Infinity" } else { "-Infinity" })
            }
            WireValue::Real(x) => serializer.serialize_f64(*x),
            WireValue::Integer(n) => serializer.serialize_i64(*n),
            WireValue::Boolean(b) => serializer.serialize_bool(*b),
            WireValue::Text(s) => serializer.serialize_str(s),
            WireValue::Enum { class, ordinal } => {
                let mut s = serializer.serialize_struct("Enum", 2)?;
                s.serialize_field("class", class)?;
                s.serialize_field("ordinal", ordinal)?;
                s.end()
            }
        }
    }
}

/// One engine diagnostic. Branch on `stage` and `code`; `message` is for people and models and
/// is not a stable contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticDto {
    pub stage: &'static str,
    pub code: String,
    pub severity: &'static str,
    pub subject: Option<String>,
    pub message: String,
}

impl DiagnosticDto {
    pub fn from_record(record: &DiagnosticRecord) -> Self {
        let key = record.key();
        Self {
            stage: stage_name(key.stage()),
            code: key.code().to_owned(),
            severity: match key.severity() {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Info => "info",
            },
            subject: match key.subject() {
                DiagnosticSubject::Absent => None,
                DiagnosticSubject::Opaque(text) => Some(text.clone()),
            },
            message: record.message().to_owned(),
        }
    }
}

pub(crate) fn stage_name(stage: DiagnosticStage) -> &'static str {
    match stage {
        DiagnosticStage::Import => "import",
        DiagnosticStage::Flatten => "flatten",
        DiagnosticStage::AttributeUnification => "attribute-unification",
        DiagnosticStage::Validation => "validation",
        DiagnosticStage::Instantiation => "instantiation",
        DiagnosticStage::Schedule => "schedule",
        DiagnosticStage::Semantics => "semantics",
        DiagnosticStage::Projection => "projection",
        DiagnosticStage::StoreRecovery => "store-recovery",
        DiagnosticStage::StoreSave => "store-save",
        DiagnosticStage::StoreInputs => "store-inputs",
        DiagnosticStage::Export => "export",
    }
}

/// An engine error reduced to a stable `kind` and human detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineErrorDto {
    pub kind: &'static str,
    pub detail: String,
}

impl EngineErrorDto {
    pub fn from_error(error: &OcError) -> Self {
        let kind = match error {
            OcError::CxfTooLarge { .. } | OcError::CxfByteLimitTooLarge { .. } => "cxf-too-large",
            OcError::Cxf(_) => "cxf",
            OcError::Validate(_) => "validate",
            OcError::Flatten(_) => "flatten",
            OcError::Build(_) => "build",
            OcError::LoadContext(_) | OcError::Load { .. } => "load",
            OcError::ParamWhileRunning { .. }
            | OcError::ParamRange { .. }
            | OcError::ParamType { .. }
            | OcError::ParamStructural { .. } => "param",
            OcError::NonFiniteTime { .. } => "non-finite-time",
            OcError::TimeRegression { .. } => "time-regression",
            OcError::ModelTimeUnrepresentable { .. } => "model-time-unrepresentable",
            OcError::UnknownPoint(_) => "unknown-point",
            OcError::InputType(_) => "input-type",
            OcError::StalePreparedFrame => "stale-prepared-frame",
            OcError::FrameSequenceExhausted => "frame-sequence-exhausted",
            OcError::FrameUnknownInput { .. } => "frame-unknown-input",
            OcError::FrameNotInput { .. } => "frame-not-input",
            OcError::FrameDuplicateInput(_) => "frame-duplicate-input",
            OcError::FrameMissingInput(_) => "frame-missing-input",
            OcError::InputDomain(_) => "input-domain",
            OcError::State(_) => "state",
            OcError::Store(_) => "store",
            // OcError is non_exhaustive; new variants land here until mapped.
            _ => "other",
        };
        Self {
            kind,
            detail: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_reals_serialize_as_strings() {
        let json = |v: WireValue| serde_json::to_string(&v).unwrap();
        assert_eq!(json(WireValue::Real(f64::NAN)), "\"NaN\"");
        assert_eq!(json(WireValue::Real(f64::INFINITY)), "\"Infinity\"");
        assert_eq!(json(WireValue::Real(f64::NEG_INFINITY)), "\"-Infinity\"");
        assert_eq!(json(WireValue::Real(1.5)), "1.5");
        assert_eq!(json(WireValue::Boolean(true)), "true");
        assert_eq!(
            json(WireValue::Enum {
                class: 1,
                ordinal: 2
            }),
            "{\"class\":1,\"ordinal\":2}"
        );
    }

    #[test]
    fn values_convert_without_loss() {
        assert_eq!(
            WireValue::from_value(&Value::Real(-0.0)),
            WireValue::Real(-0.0)
        );
        assert_eq!(
            WireValue::from_value(&Value::Integer(7)),
            WireValue::Integer(7)
        );
        assert_eq!(
            WireValue::from_value(&Value::Boolean(false)),
            WireValue::Boolean(false)
        );
    }
}
