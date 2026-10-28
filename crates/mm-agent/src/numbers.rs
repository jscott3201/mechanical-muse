//! A check that model-written text only states numbers it was given. The model may quote a
//! number, rounded, but must not compute one: a sum, difference or percentage of its own is
//! reported here and the reply is refused.
//!
//! What passes:
//! - a number in the facts, written with fewer decimals, or rounded to tens, hundreds, ...
//!   as long as at least two significant figures remain ("about 850 kg" for 848, "4,400 kWh"
//!   for 4,395, "$270" for 268.56; not "1,000" for 848);
//! - a duration the facts give as steps: every `steps` count times the run's `step_s`, in
//!   minutes and in hours ("20 minutes" for 10 two-minute steps);
//! - small whole numbers (0 to 12): counts, floors and hours that plain sentences use ("three
//!   sequences", "floor 1"); refusing them would reject honest replies.

use serde_json::Value;

/// Numbers in `text` as written (`1,644`, `0.119`), in order. A comma is a thousands separator
/// only when exactly three digits follow it, so "1,2,3" is three numbers. A digit run inside an
/// identifier (`floor1_reaAHU`) counts too, so identifiers quoted from the facts pass.
fn numbers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !digit(i) {
            i += 1;
            continue;
        }
        let start = i;
        let mut seen_point = false;
        while let Some(c) = chars.get(i) {
            if c.is_ascii_digit() {
                i += 1;
            } else if *c == ','
                && !seen_point
                && digit(i + 1)
                && digit(i + 2)
                && digit(i + 3)
                && !digit(i + 4)
            {
                i += 4;
            } else if *c == '.' && !seen_point && digit(i + 1) {
                seen_point = true;
                i += 1;
            } else {
                break;
            }
        }
        out.push(
            chars
                .get(start..i)
                .map_or_else(String::new, |s| s.iter().collect()),
        );
    }
    out
}

/// A written number: its value and the place it is given to (2 for "0.12", 0 for "848", -1 for
/// "850", -2 for "4,400"), and its significant figures.
struct Written {
    value: f64,
    place: i32,
    significant: usize,
}

fn parse(written: &str) -> Option<Written> {
    let plain: String = written.chars().filter(|c| *c != ',').collect();
    let value = plain.parse::<f64>().ok()?;
    let (int, frac) = plain.split_once('.').unwrap_or((plain.as_str(), ""));
    let int = int.trim_start_matches('0');
    let (place, significant) = if frac.is_empty() {
        let kept = int.trim_end_matches('0');
        let zeros = int.len() - kept.len();
        (-i32::try_from(zeros).ok()?, kept.len())
    } else {
        let digits = format!("{int}{frac}");
        (
            i32::try_from(frac.len()).ok()?,
            digits.trim_start_matches('0').len(),
        )
    };
    Some(Written {
        value,
        place,
        significant,
    })
}

/// `fact` rounded to `w`'s place reads as `w` (signs ignored: "848 kg less" quotes -848).
fn quotes(fact: f64, w: &Written) -> bool {
    let place = w.place.clamp(-12, 9);
    let scale = 10f64.powi(place);
    let exact = ((fact.abs() * scale).round() - (w.value * scale).round()).abs() < 0.5;
    // Rounded to tens or coarser: only with two significant figures left.
    exact && (place >= 0 || w.significant >= 2 || (fact.abs() - w.value).abs() < 1e-9)
}

fn collect(
    v: &Value,
    key: Option<&str>,
    nums: &mut Vec<f64>,
    steps: &mut Vec<f64>,
    step_s: &mut Vec<f64>,
) {
    match v {
        Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                nums.push(x);
                match key {
                    Some("steps") => steps.push(x),
                    Some("step_s") => step_s.push(x),
                    _ => {}
                }
            }
        }
        Value::String(s) => {
            nums.extend(numbers(s).iter().filter_map(|n| parse(n)).map(|w| w.value))
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, key, nums, steps, step_s)),
        Value::Object(o) => o
            .iter()
            .for_each(|(k, x)| collect(x, Some(k.as_str()), nums, steps, step_s)),
        Value::Bool(_) | Value::Null => {}
    }
    if let Some(k) = key {
        nums.extend(numbers(k).iter().filter_map(|n| parse(n)).map(|w| w.value));
    }
}

/// Every number the facts give, and the durations their step counts stand for.
fn known(facts: &Value) -> Vec<f64> {
    let (mut nums, mut steps, mut step_s) = (Vec::new(), Vec::new(), Vec::new());
    collect(facts, None, &mut nums, &mut steps, &mut step_s);
    if step_s.is_empty() {
        step_s.push(120.0);
    }
    for n in &steps {
        for s in &step_s {
            nums.extend([n * s / 60.0, n * s / 3600.0]);
        }
    }
    nums
}

/// Numbers in `texts` that the facts do not give, in order of appearance, each once.
pub fn unsupported_numbers(texts: &[&str], facts: &Value) -> Vec<String> {
    let known = known(facts);
    let mut bad: Vec<String> = Vec::new();
    for text in texts {
        for written in numbers(text) {
            let Some(w) = parse(&written) else { continue };
            let small = w.place >= 0 && !written.contains('.') && w.value <= 12.0;
            if !small && !known.iter().any(|f| quotes(*f, &w)) && !bad.contains(&written) {
                bad.push(written);
            }
        }
    }
    bad
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn numbers_are_read_as_written() {
        assert_eq!(
            numbers("1,644 kg, then 0.119 at day 186 00:40; floor1_reaAHU and 3."),
            ["1,644", "0.119", "186", "00", "40", "1", "3"]
        );
        assert_eq!(
            numbers("steps 1,2,3 and 12,34"),
            ["1", "2", "3", "12", "34"]
        );
        assert_eq!(numbers("1,644,000 W"), ["1,644,000"]);
        let w = parse("4,400").unwrap();
        assert_eq!((w.value, w.place, w.significant), (4400.0, -2, 2));
        let w = parse("0.12").unwrap();
        assert_eq!((w.place, w.significant), (2, 2));
    }

    fn facts() -> Value {
        json!({"run": {"steps": 240, "step_s": 120.0, "from": "day 143 06:00"},
               "events": [{"when": "day 143 08:00", "steps": 10}],
               "ledger": {"scope1_kg_co2e": 796.0, "change_kg_co2e": -848.0, "kwh": 4395.0,
                          "usd": -268.56, "tdis": 0.119}})
    }

    #[test]
    fn quoted_rounded_and_small_numbers_pass() {
        let ok = "Gas fell by 848 kg (about 850 kg) to 796 kg, 4,400 kWh, about $270 less, on \
                  day 143 from 06:00 over 240 steps; discomfort was 0.12.";
        assert!(
            unsupported_numbers(&[ok], &facts()).is_empty(),
            "{:?}",
            unsupported_numbers(&[ok], &facts())
        );
    }

    #[test]
    fn durations_follow_from_step_counts() {
        let ok = "The fault lasted 20 minutes; the run covered 8 hours, or 480 minutes.";
        assert!(
            unsupported_numbers(&[ok], &facts()).is_empty(),
            "{:?}",
            unsupported_numbers(&[ok], &facts())
        );
        assert_eq!(
            unsupported_numbers(&["it lasted 25 minutes"], &facts()),
            ["25"]
        );
    }

    #[test]
    fn invented_or_too_coarse_numbers_do_not() {
        let invented = "That is a 52% cut, or 1,234 kWh, about 1,000 kg.";
        assert_eq!(
            unsupported_numbers(&["fine", invented], &facts()),
            ["52", "1,234", "1,000"]
        );
        assert_eq!(unsupported_numbers(&["0.5 K warmer"], &facts()), ["0.5"]);
        // A list is not a thousands-separated number.
        assert_eq!(
            unsupported_numbers(&["floors 1,2,3"], &facts()),
            Vec::<String>::new()
        );
    }
}
