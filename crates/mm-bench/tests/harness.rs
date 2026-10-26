//! Card parsing, scrubbing and baselines on the committed library fixtures (offline).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_bench::{
    Rule, Tier, baseline_graph, baselines, find_leaks, leak_tokens, load_rule_files, prompt_hash,
};
use mm_graph::{Palette, validate};

const FIXTURES: [(&str, &str); 6] = [
    ("AHU-0027", "ahu"),
    ("AHU-0005", "ahu"),
    ("HW-0011", "hw"),
    ("RTU-0007", "rtu"),
    ("HP-0005", "hp"),
    ("AHU-0023", "ahu"),
];

fn rule(id: &str, family: &str) -> Rule {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/open-control-library");
    load_rule_files(
        id,
        &dir.join(id).join("card.md"),
        &dir.join(id).join("rule.cxf.jsonld"),
        &dir.join(id).join("vectors.json"),
        &dir.join("points").join(format!("{family}.points.json")),
    )
    .unwrap()
}

#[test]
fn cards_parse_without_their_answer_fields() {
    let r = rule("AHU-0005", "ahu");
    assert_eq!(r.card.id, "AHU-0005");
    assert_eq!(r.card.points, ["mat", "sat"]);
    assert_eq!(r.card.outputs[0].name, "yFault");
    assert!(r.card.params.contains_key("alarm_delay"));
    assert!(r.card.logic.as_deref().unwrap().contains("mat"));
    assert!(!r.card.description.is_empty());
}

#[test]
fn scrubbed_tasks_never_leak() {
    let palette = Palette::pinned().unwrap();
    for (id, family) in FIXTURES {
        let r = rule(id, family);
        let tokens = leak_tokens(&palette, &r.reference);
        for tier in [Tier::A, Tier::B] {
            let Ok(scrubbed) = r.scrubbed(&palette, tier) else {
                assert_eq!(tier, Tier::B, "{id}: tier A must always scrub");
                continue;
            };
            let text = mm_agent::task_message(&scrubbed.task);
            assert_eq!(
                find_leaks(&text, &tokens),
                Vec::<String>::new(),
                "{id} {tier:?}"
            );
            assert!(!text.contains("cxf"), "{id}");
            assert!(!text.contains("diagram"), "{id}");
            // The interface is the rule's own, so a correct graph can pass the vectors.
            let names: Vec<_> = scrubbed
                .task
                .inputs
                .iter()
                .map(|p| p.name.clone())
                .collect();
            let want: Vec<_> = r.reference.inputs.iter().map(|p| p.name.clone()).collect();
            assert_eq!(names, want, "{id}");
            assert_eq!(scrubbed.task.logic.is_some(), tier == Tier::B, "{id}");
        }
    }
}

#[test]
fn leak_matching_is_whole_word() {
    let tokens = vec![
        "TrueDelay".to_owned(),
        "`gap`".to_owned(),
        "htgThr".to_owned(),
    ];
    assert_eq!(find_leaks("uses a TrueDelay here", &tokens), ["TrueDelay"]);
    assert!(find_leaks("TrueDelayed is not it", &tokens).is_empty());
    assert!(find_leaks("the gap is large", &tokens).is_empty());
    assert_eq!(find_leaks("the `gap` block", &tokens), ["`gap`"]);
    assert_eq!(find_leaks("(htgThr.t)", &tokens), ["htgThr"]);
}

#[test]
fn hw_0011_tier_b_is_skipped_for_naming_classes() {
    // Its pseudo-code names MovingAverage and TrueDelay, so tier B must refuse it.
    let palette = Palette::pinned().unwrap();
    let r = rule("HW-0011", "hw");
    assert!(r.scrubbed(&palette, Tier::B).is_err());
    assert!(r.scrubbed(&palette, Tier::A).is_ok());
}

#[test]
fn baselines_validate_and_never_pass_a_whole_rule() {
    let palette = Palette::pinned().unwrap();
    let rules: Vec<Rule> = FIXTURES.iter().map(|(id, f)| rule(id, f)).collect();
    for r in &rules {
        for value in [false, true] {
            let g = baseline_graph(&r.reference, value);
            assert_eq!(validate(&g, &palette), vec![], "{} {value}", r.id);
        }
    }
    let (f, t) = baselines(&rules, &palette).unwrap();
    let scenarios: usize = rules.iter().map(|r| r.vectors.scenarios.len()).sum();
    assert_eq!((f.scenarios, t.scenarios), (scenarios, scenarios));
    // Every library rule has at least one scenario that expects the fault to fire.
    assert_eq!(f.rules_passed, 0);
    assert!(f.scenarios_passed > 0 && f.scenarios_passed < scenarios);
}

#[test]
fn prompt_hash_is_stable() {
    assert_eq!(prompt_hash("abc"), prompt_hash("abc"));
    assert_ne!(prompt_hash("abc"), prompt_hash("abd"));
    assert!(prompt_hash("").starts_with("fnv1a64:"));
}

#[test]
fn lowercase_class_names_and_rule_ids_are_leaks() {
    let palette = Palette::pinned().unwrap();
    let tokens = leak_tokens(&palette, &rule("AHU-0005", "ahu").reference);
    let leaks = |text: &str| find_leaks(text, &tokens);
    assert_eq!(
        leaks("Rescales the moving average of the pulses"),
        ["moving average"]
    );
    assert_eq!(leaks("a Moving-Average stage"), ["moving average"]);
    assert_eq!(leaks("then a true delay of 900 s"), ["true delay"]);
    assert_eq!(leaks("same graph as AHU-0017 reversed"), ["AHU-0017"]);
    assert!(leaks("the average flow and a delay after start").is_empty());
}
