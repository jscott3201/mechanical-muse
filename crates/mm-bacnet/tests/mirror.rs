//! The mirror on a real B/IP socket, read and written by rusty-bacnet's own client.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use bacnet_client::client::BACnetClient;
use bacnet_encoding::primitives::{
    decode_application_value, encode_app_boolean, encode_app_character_string, encode_app_real,
};
use bacnet_transport::bip::BipTransport;
use bacnet_types::constructed::{PropertyReference, ReadAccessSpecification};
use bacnet_types::enums::{ErrorClass, ErrorCode, ObjectType, PropertyIdentifier, Reliability};
use bacnet_types::error::Error;
use bacnet_types::primitives::{ObjectIdentifier, PropertyValue};
use mm_bacnet::{Mirror, MirrorConfig, PointMap, PointSpec, Source, quantize};

fn config() -> MirrorConfig {
    MirrorConfig {
        device_instance: 599,
        device_name: "MM-MIRROR-TEST".into(),
        interface: Ipv4Addr::LOCALHOST,
        port: 0,
        broadcast: Ipv4Addr::LOCALHOST,
    }
}

fn map() -> PointMap {
    PointMap::new(vec![
        PointSpec {
            name: "reaChiWatSys_TCHWSup_y".into(),
            source: Source::Measured,
            unit: "K".into(),
        },
        PointSpec {
            name: "chw.TSupSet".into(),
            source: Source::Derived,
            unit: "K".into(),
        },
        PointSpec {
            name: "oveChiWatSys_dpSet_u".into(),
            source: Source::Commanded,
            unit: "Pa".into(),
        },
    ])
    .unwrap()
}

async fn client() -> BACnetClient<BipTransport> {
    BACnetClient::bip_builder()
        .interface(Ipv4Addr::LOCALHOST)
        .port(0)
        .broadcast_address(Ipv4Addr::LOCALHOST)
        .apdu_timeout_ms(2000)
        .build()
        .await
        .unwrap()
}

async fn read(
    client: &BACnetClient<BipTransport>,
    mac: &[u8],
    oid: ObjectIdentifier,
    property: PropertyIdentifier,
) -> PropertyValue {
    let ack = client
        .read_property(mac, oid, property, None)
        .await
        .unwrap();
    decode_application_value(&ack.property_value, 0).unwrap().0
}

fn ai(i: u32) -> ObjectIdentifier {
    ObjectIdentifier::new(ObjectType::ANALOG_INPUT, i).unwrap()
}
fn av(i: u32) -> ObjectIdentifier {
    ObjectIdentifier::new(ObjectType::ANALOG_VALUE, i).unwrap()
}

#[tokio::test]
async fn peers_read_the_values_and_cannot_write_them() {
    let mut mirror = Mirror::start(&config(), map()).await.unwrap();
    let mut client = client().await;
    let mac = mirror.local_mac().to_vec();

    let values = BTreeMap::from([
        ("reaChiWatSys_TCHWSup_y".to_owned(), 279.123_456_789),
        ("chw.TSupSet".to_owned(), 278.71),
        ("oveChiWatSys_dpSet_u".to_owned(), 239_125.0),
    ]);
    let report = mirror.update(&values).await;
    assert_eq!(report.updated, 3);
    assert!(report.no_data.is_empty());

    // f64 is quantized to f32 once; a peer reads exactly that f32.
    assert_eq!(
        read(&client, &mac, ai(1), PropertyIdentifier::PRESENT_VALUE).await,
        PropertyValue::Real(quantize(279.123_456_789))
    );
    // Commanded and derived points are Analog Values 1 and 2, ranked by name.
    assert_eq!(
        read(&client, &mac, av(1), PropertyIdentifier::PRESENT_VALUE).await,
        PropertyValue::Real(278.71)
    );
    assert_eq!(
        read(&client, &mac, av(2), PropertyIdentifier::OBJECT_NAME).await,
        PropertyValue::CharacterString("oveChiWatSys_dpSet_u".into())
    );

    // Writes the objects would accept under the library's default policy (Out_Of_Service,
    // then Object_Name) are refused by the device's deny-all policy, with that policy's error.
    let denied = |r: &Result<(), Error>| {
        matches!(
            r,
            Err(Error::Protocol { class, code })
                if *class == ErrorClass::SERVICES && *code == ErrorCode::SERVICE_REQUEST_DENIED
        )
    };
    for oid in [ai(1), av(1), av(2)] {
        let mut oos = bytes::BytesMut::new();
        encode_app_boolean(&mut oos, true);
        let r = client
            .write_property(
                &mac,
                oid,
                PropertyIdentifier::OUT_OF_SERVICE,
                None,
                oos.to_vec(),
                None,
            )
            .await;
        assert!(denied(&r), "{oid:?} out of service: {r:?}");
        let mut name = bytes::BytesMut::new();
        encode_app_character_string(&mut name, "renamed").unwrap();
        let r = client
            .write_property(
                &mac,
                oid,
                PropertyIdentifier::OBJECT_NAME,
                None,
                name.to_vec(),
                None,
            )
            .await;
        assert!(denied(&r), "{oid:?} name: {r:?}");
        let mut pv = bytes::BytesMut::new();
        encode_app_real(&mut pv, 300.0);
        let r = client
            .write_property(
                &mac,
                oid,
                PropertyIdentifier::PRESENT_VALUE,
                None,
                pv.to_vec(),
                Some(8),
            )
            .await;
        assert!(
            matches!(r, Err(Error::Protocol { .. })),
            "{oid:?} value: {r:?}"
        );
    }
    assert_eq!(
        read(&client, &mac, av(1), PropertyIdentifier::PRESENT_VALUE).await,
        PropertyValue::Real(278.71)
    );
    assert_eq!(
        read(&client, &mac, av(2), PropertyIdentifier::OBJECT_NAME).await,
        PropertyValue::CharacterString("oveChiWatSys_dpSet_u".into())
    );
    // Local updates still apply after the refusals.
    let mut next = values.clone();
    next.insert("chw.TSupSet".into(), 279.0);
    assert_eq!(mirror.update(&next).await.updated, 3);
    assert_eq!(
        read(&client, &mac, av(1), PropertyIdentifier::PRESENT_VALUE).await,
        PropertyValue::Real(279.0)
    );

    client.stop().await.unwrap();
    mirror.stop().await.unwrap();
}

#[tokio::test]
async fn a_missing_or_nan_value_reads_as_no_sensor_until_it_returns() {
    let mut mirror = Mirror::start(&config(), map()).await.unwrap();
    let mut client = client().await;
    let mac = mirror.local_mac().to_vec();
    let good = BTreeMap::from([
        ("reaChiWatSys_TCHWSup_y".to_owned(), 279.0),
        ("chw.TSupSet".to_owned(), 278.71),
    ]);
    mirror.update(&good).await;

    let mut faulty = good.clone();
    faulty.insert("reaChiWatSys_TCHWSup_y".into(), f64::NAN);
    let report = mirror.update(&faulty).await;
    // In map order: Analog Inputs, then Analog Values.
    assert_eq!(
        report.no_data,
        ["reaChiWatSys_TCHWSup_y", "oveChiWatSys_dpSet_u"]
    );
    assert_eq!(
        read(&client, &mac, ai(1), PropertyIdentifier::RELIABILITY).await,
        PropertyValue::Enumerated(Reliability::NO_SENSOR.to_raw())
    );
    // The last good value stays, flagged; it is never replaced by NaN.
    assert_eq!(
        read(&client, &mac, ai(1), PropertyIdentifier::PRESENT_VALUE).await,
        PropertyValue::Real(279.0)
    );

    mirror.update(&good).await;
    assert_eq!(
        read(&client, &mac, ai(1), PropertyIdentifier::RELIABILITY).await,
        PropertyValue::Enumerated(Reliability::NO_FAULT_DETECTED.to_raw())
    );

    client.stop().await.unwrap();
    mirror.stop().await.unwrap();
}

#[tokio::test]
async fn nothing_reads_as_good_before_the_first_value() {
    let mut mirror = Mirror::start(&config(), map()).await.unwrap();
    let mut client = client().await;
    let mac = mirror.local_mac().to_vec();
    for oid in [ai(1), av(1), av(2)] {
        assert_eq!(
            read(&client, &mac, oid, PropertyIdentifier::RELIABILITY).await,
            PropertyValue::Enumerated(Reliability::NO_SENSOR.to_raw()),
            "{oid:?}"
        );
    }
    // A finite f64 beyond f32's range is no data, not infinity; the other points still update.
    let report = mirror
        .update(&BTreeMap::from([
            ("reaChiWatSys_TCHWSup_y".to_owned(), 1e39),
            ("chw.TSupSet".to_owned(), 278.71),
        ]))
        .await;
    assert_eq!(report.updated, 1);
    assert_eq!(
        report.no_data,
        ["reaChiWatSys_TCHWSup_y", "oveChiWatSys_dpSet_u"]
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    client.stop().await.unwrap();
    mirror.stop().await.unwrap();
}

#[tokio::test]
async fn one_read_of_every_object_is_answered_in_segments() {
    // 30 points, five properties each: well past one APDU.
    let specs = (1..=30)
        .map(|i| PointSpec {
            name: format!("point_{i:02}_y"),
            source: if i % 2 == 0 {
                Source::Measured
            } else {
                Source::Derived
            },
            unit: "K".into(),
        })
        .collect();
    let mut mirror = Mirror::start(&config(), PointMap::new(specs).unwrap())
        .await
        .unwrap();
    let mut client = client().await;
    let mac = mirror.local_mac().to_vec();
    let props = [
        PropertyIdentifier::OBJECT_NAME,
        PropertyIdentifier::PRESENT_VALUE,
        PropertyIdentifier::RELIABILITY,
        PropertyIdentifier::STATUS_FLAGS,
        PropertyIdentifier::UNITS,
    ];
    let request: Vec<ReadAccessSpecification> = (1..=15)
        .flat_map(|i| [ai(i), av(i)])
        .map(|object_identifier| ReadAccessSpecification {
            object_identifier,
            list_of_property_references: props
                .iter()
                .map(|&property_identifier| PropertyReference {
                    property_identifier,
                    property_array_index: None,
                })
                .collect(),
        })
        .collect();
    let ack = client.read_property_multiple(&mac, request).await.unwrap();
    assert_eq!(ack.list_of_read_access_results.len(), 30);
    assert!(
        ack.list_of_read_access_results
            .iter()
            .all(|r| r.list_of_results.len() == props.len())
    );
    client.stop().await.unwrap();
    mirror.stop().await.unwrap();
}
