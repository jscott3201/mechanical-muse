//! Frame stepping that enforces the engine's contract on the host side: every boundary input,
//! exactly once, with its declared type, on every frame. Values are held in a table the caller
//! updates; the engine itself never holds a value.

use std::collections::BTreeMap;

use oce_api::{CompletedFrame, Engine, InputDefinition, Value, ValueType};

use crate::dto::{EngineErrorDto, WireValue};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StepError {
    #[error("input {0} has no value yet")]
    Unset(String),
    #[error("{0} is not a boundary input")]
    UnknownInput(String),
    #[error("{path} expects {expected}")]
    WrongType {
        path: String,
        expected: &'static str,
    },
    #[error("engine refused the frame: {}", .0.detail)]
    Engine(EngineErrorDto),
}

impl StepError {
    pub fn code(&self) -> &'static str {
        match self {
            StepError::Unset(_) => "unset-input",
            StepError::UnknownInput(_) => "unknown-input",
            StepError::WrongType { .. } => "input-type-mismatch",
            StepError::Engine(e) => e.kind,
        }
    }
}

/// Owns one engine and the current value of each boundary input.
pub struct FrameStepper {
    engine: Engine,
    definitions: Vec<InputDefinition>,
    values: BTreeMap<String, Value>,
}

impl FrameStepper {
    pub fn new(engine: Engine) -> Result<Self, StepError> {
        let definitions = engine
            .input_definitions()
            .map_err(|e| StepError::Engine(EngineErrorDto::from_error(&e)))?;
        Ok(Self {
            engine,
            definitions,
            values: BTreeMap::new(),
        })
    }

    /// Canonical input paths, in the engine's order.
    pub fn input_paths(&self) -> Vec<String> {
        self.definitions.iter().map(|d| d.path.clone()).collect()
    }

    pub fn input_type(&self, path: &str) -> Option<ValueType> {
        self.definitions
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.value_type)
    }

    /// The declared type of an input, in words (`"a Real"`, `"an Integer"`, ...), for hosts
    /// that check their bindings without naming engine types.
    pub fn input_type_name(&self, path: &str) -> Option<&'static str> {
        self.input_type(path).map(type_name)
    }

    /// Sets the value an input will carry from the next frame on.
    pub fn set(&mut self, path: &str, value: Value) -> Result<(), StepError> {
        let ty = self
            .input_type(path)
            .ok_or_else(|| StepError::UnknownInput(path.to_owned()))?;
        let fits = matches!(
            (ty, &value),
            (ValueType::Real, Value::Real(_))
                | (ValueType::Integer, Value::Integer(_))
                | (ValueType::Boolean, Value::Boolean(_))
                | (ValueType::Enum(_), Value::Enum { .. })
        );
        if !fits {
            return Err(StepError::WrongType {
                path: path.to_owned(),
                expected: type_name(ty),
            });
        }
        self.values.insert(path.to_owned(), value);
        Ok(())
    }

    /// Sets an input from a wire value, converted to the input's declared type. This is how
    /// hosts outside `mm-bridge` drive the engine without naming its types.
    pub fn set_wire(&mut self, path: &str, value: &WireValue) -> Result<(), StepError> {
        let ty = self
            .input_type(path)
            .ok_or_else(|| StepError::UnknownInput(path.to_owned()))?;
        let converted = match (ty, value) {
            (ValueType::Real, WireValue::Real(x)) => Some(Value::Real(*x)),
            (ValueType::Integer, WireValue::Integer(n)) => Some(Value::Integer(*n)),
            (ValueType::Boolean, WireValue::Boolean(b)) => Some(Value::Boolean(*b)),
            (ValueType::Enum(class), WireValue::Enum { ordinal, .. }) => Some(Value::Enum {
                class,
                ordinal: *ordinal,
            }),
            _ => None,
        };
        match converted {
            Some(v) => self.set(path, v),
            None => Err(StepError::WrongType {
                path: path.to_owned(),
                expected: type_name(ty),
            }),
        }
    }

    /// Like [`FrameStepper::step`], returning the boundary outputs as wire values, in the
    /// engine's (lexical) path order.
    pub fn step_wire(&mut self, time: f64) -> Result<Vec<(String, WireValue)>, StepError> {
        let frame = self.step(time)?;
        Ok(frame
            .outputs()
            .iter()
            .map(|(path, value)| (path.clone(), WireValue::from_value(value)))
            .collect())
    }

    /// Submits the whole input table at `time` and executes one frame. A refusal leaves the
    /// engine unchanged; an accepted frame is a state transition and must never be resubmitted.
    pub fn step(&mut self, time: f64) -> Result<CompletedFrame, StepError> {
        let mut entries = Vec::with_capacity(self.definitions.len());
        for def in &self.definitions {
            let value = self
                .values
                .get(&def.path)
                .ok_or_else(|| StepError::Unset(def.path.clone()))?;
            entries.push((def.path.as_str(), value.clone()));
        }
        let engine_err = |e: oce_api::OcError| StepError::Engine(EngineErrorDto::from_error(&e));
        let prepared = self
            .engine
            .prepare_frame(time, &entries)
            .map_err(engine_err)?;
        self.engine.execute_frame(prepared).map_err(engine_err)
    }
}

pub(crate) fn type_name(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Real => "a Real",
        ValueType::Integer => "an Integer",
        ValueType::Boolean => "a Boolean",
        ValueType::String => "a String",
        ValueType::Enum(_) => "an Enum ordinal",
    }
}
