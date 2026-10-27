//! A read-only BACnet/IP device that mirrors a live run's points. Peers can discover it and read
//! every object; every remote write is refused (`MutationPolicy::DenyAll`). Values are updated
//! with the server's local calls only.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use bacnet_objects::DeviceIdentity;
use bacnet_objects::analog::{AnalogInputObject, AnalogValueObject};
use bacnet_objects::database::ObjectDatabase;
use bacnet_objects::device::{DeviceConfig, DeviceObject};
use bacnet_objects::present_value_access::PresentValueAccess;
use bacnet_objects::traits::BACnetObject;
use bacnet_server::mutation::MutationPolicy;
use bacnet_server::server::BACnetServer;
use bacnet_transport::bip::BipTransport;
use bacnet_types::enums::{ObjectType, PropertyIdentifier, Reliability, Segmentation};
use bacnet_types::primitives::{ObjectIdentifier, PropertyValue};
use serde::Serialize;

use crate::map::{PointMap, Source};

/// A placeholder vendor id: this project has no assigned BACnet vendor id. Not 0, which is
/// the standard body's own; the top of the range, so it reads as no vendor in particular.
pub const VENDOR_ID: u16 = u16::MAX;

#[derive(Debug, Clone)]
pub struct MirrorConfig {
    pub device_instance: u32,
    pub device_name: String,
    pub interface: Ipv4Addr,
    /// 47808 for a demo; 0 picks a free port (tests).
    pub port: u16,
    pub broadcast: Ipv4Addr,
}

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("BACnet: {0}")]
    Bacnet(#[from] bacnet_types::error::Error),
}

/// What one update did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateReport {
    pub updated: usize,
    /// Points with no finite value this time: they keep their last value and read as
    /// unreliable (no sensor) until a value arrives.
    pub no_data: Vec<String>,
    /// Points the device refused to update (`name: error`); each also reads as no sensor.
    pub errors: Vec<String>,
}

pub struct Mirror {
    server: BACnetServer<BipTransport>,
    map: PointMap,
    device: u32,
}

impl Mirror {
    /// Builds the device and its objects and starts serving.
    pub async fn start(config: &MirrorConfig, map: PointMap) -> Result<Self, MirrorError> {
        // Segmented replies: a client reading every object in one ReadPropertyMultiple (as
        // tools browsing a device do) gets an answer longer than one APDU.
        let identity = DeviceIdentity::new(config.device_instance, VENDOR_ID)?
            .with_name(config.device_name.clone())
            .with_segmentation(Segmentation::BOTH);
        let mut device = DeviceObject::new(DeviceConfig {
            identity: identity.clone(),
            vendor_name: "Mechanical Muse (placeholder vendor id)".into(),
            model_name: "read-only mirror".into(),
            ..DeviceConfig::default()
        })?;
        let mut db = ObjectDatabase::new();
        let mut list = vec![device.object_identifier()];
        for point in &map.points {
            let object: Box<dyn BACnetObject> = match point.source {
                Source::Measured => {
                    let mut ai = AnalogInputObject::new(point.instance, &point.name, point.units)?;
                    ai.set_description(&point.description);
                    Box::new(ai)
                }
                Source::Derived | Source::Commanded => {
                    let mut av = AnalogValueObject::with_access(
                        point.instance,
                        &point.name,
                        point.units,
                        PresentValueAccess::ReadOnly,
                    )?;
                    av.set_description(&point.description);
                    Box::new(av)
                }
            };
            list.push(object.object_identifier());
            db.add(object)?;
        }
        device.set_object_list(list);
        db.add(Box::new(device))?;
        let server = BACnetServer::bip_builder()
            .interface(config.interface)
            .port(config.port)
            .broadcast_address(config.broadcast)
            .device(identity)
            .database(db)
            .mutation_policy(MutationPolicy::DenyAll)
            .build()
            .await?;
        let mirror = Self {
            server,
            map,
            device: config.device_instance,
        };
        // New objects start at 0.0 and reliable; nothing has produced a value yet.
        mirror.mark_all_no_data().await;
        Ok(mirror)
    }

    pub fn map(&self) -> &PointMap {
        &self.map
    }

    pub fn device_instance(&self) -> u32 {
        self.device
    }

    /// The device's B/IP address (IPv4 and port, 6 bytes).
    pub fn local_mac(&self) -> &[u8] {
        self.server.local_mac()
    }

    /// Sets every mapped point found in `values`. A point that is absent or not finite keeps
    /// its last value and is flagged unreliable, so a peer never reads a stale value as good.
    /// Never fails as a whole: per-point problems are in the report.
    pub async fn update(&self, values: &BTreeMap<String, f64>) -> UpdateReport {
        let mut report = UpdateReport::default();
        for point in &self.map.points {
            // Checked after the conversion: a finite f64 beyond f32's range is not a value.
            let value = values
                .get(&point.name)
                .map(|v| quantize(*v))
                .filter(|v| v.is_finite());
            match self.set(point.source, point.instance, value).await {
                Ok(true) => report.updated += 1,
                Ok(false) => report.no_data.push(point.name.clone()),
                Err(e) => {
                    // One point's failure never leaves the others stale: flag it and go on.
                    let _ = self.set(point.source, point.instance, None).await;
                    report.errors.push(format!("{}: {e}", point.name));
                }
            }
        }
        report
    }

    /// Every point to no data: before the first run, and after each run ends, so a peer never
    /// reads a value nothing is producing as good.
    pub async fn mark_all_no_data(&self) -> UpdateReport {
        self.update(&BTreeMap::new()).await
    }

    /// `Ok(true)` when `value` was set, `Ok(false)` when the point was flagged no sensor.
    async fn set(
        &self,
        source: Source,
        instance: u32,
        value: Option<f32>,
    ) -> Result<bool, MirrorError> {
        let oid = ObjectIdentifier::new(source.object_type(), instance)?;
        match value {
            Some(v) => {
                self.server
                    .set_present_value_local(&oid, PropertyValue::Real(v))
                    .await?;
                self.reliability(&oid, Reliability::NO_FAULT_DETECTED)
                    .await?;
                Ok(true)
            }
            None => {
                self.reliability(&oid, Reliability::NO_SENSOR).await?;
                Ok(false)
            }
        }
    }

    /// Reliability is the object's own assessment, set through its internal evaluation hook
    /// (a remote or local property write is refused while the object is in service).
    async fn reliability(
        &self,
        oid: &ObjectIdentifier,
        value: Reliability,
    ) -> Result<(), MirrorError> {
        let mut db = self.server.database().write().await;
        if let Some(object) = db.get_mut(oid) {
            object.set_reliability_internal(value)?;
        }
        Ok(())
    }

    /// A point's present value and whether it is reliable, read from the device's own
    /// database (what a peer would read).
    pub async fn value(&self, name: &str) -> Option<(f32, bool)> {
        let point = self.map.get(name)?;
        let oid = ObjectIdentifier::new(point.source.object_type(), point.instance).ok()?;
        let db = self.server.database().read().await;
        let object = db.get(&oid)?;
        let value = match object
            .read_property(PropertyIdentifier::PRESENT_VALUE, None)
            .ok()?
        {
            PropertyValue::Real(v) => v,
            _ => return None,
        };
        let reliable = matches!(
            object.read_property(PropertyIdentifier::RELIABILITY, None).ok()?,
            PropertyValue::Enumerated(r) if r == Reliability::NO_FAULT_DETECTED.to_raw()
        );
        Some((value, reliable))
    }

    /// Stops serving. Call before dropping.
    pub async fn stop(&mut self) -> Result<(), MirrorError> {
        self.server.stop().await?;
        Ok(())
    }
}

/// Engine and BOPTEST values are `f64`; BACnet REAL is `f32`. This is the only conversion.
#[allow(clippy::cast_possible_truncation)]
pub fn quantize(value: f64) -> f32 {
    value as f32
}

/// The object id a point maps to, for callers that read the device.
pub fn object_id(object_type: ObjectType, instance: u32) -> Option<ObjectIdentifier> {
    ObjectIdentifier::new(object_type, instance).ok()
}
