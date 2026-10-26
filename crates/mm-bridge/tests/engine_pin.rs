//! Smoke test for the pinned engine: the catalog the palette and the judge share.

#[test]
fn catalog_has_the_136_elementary_classes() {
    assert_eq!(oce_api::catalog().len(), 136);
}

#[test]
fn catalog_content_id_is_stable() {
    let first = oce_api::catalog_content_id(oce_api::catalog());
    let second = oce_api::catalog_content_id(oce_api::catalog());
    assert!(
        first.starts_with("catalog:1:fnv1a128:"),
        "unexpected id {first}"
    );
    assert_eq!(first, second);
}
