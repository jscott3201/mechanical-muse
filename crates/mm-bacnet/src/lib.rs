//! A read-only BACnet/IP mirror of a live run's points, on rusty-bacnet (pre-existing, pinned).
//! BOPTEST readings are Analog Inputs; sequence outputs and the setpoints sent to BOPTEST are
//! read-only Analog Values. Remote writes are refused; values change only through local calls.

#![forbid(unsafe_code)]

mod map;
mod mirror;

pub use map::{MapError, MappedPoint, PointMap, PointSpec, Source, boptest_unit, units};
pub use mirror::{Mirror, MirrorConfig, MirrorError, UpdateReport, VENDOR_ID, object_id, quantize};
