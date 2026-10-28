//! A check that model-written text only states numbers it was given. The model may quote a
//! number (rounded to fewer decimals is fine) but must not compute one: a sum, difference or
//! percentage of its own is reported here and the reply is refused.
//!
//! Small whole numbers (0 to 12) always pass: they are counts, floors, hours and steps that
//! plain sentences use ("three sequences", "floor 1", "2 hours" for 60 two-minute steps), and
//! refusing them would reject honest replies. Anything else must match a number in the facts.

/// Numbers in `text` as written (`1,644`, `0.119`, `-848`), in order. A digit run that is part
/// of an identifier (`floor1_reaAHU`) counts too, so identifiers quoted from the facts pass.
fn numbers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let Some(c) = chars.get(i) else { break };
        if !c.is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while let Some(c) = chars.get(i) {
            let next_is_digit = chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if c.is_ascii_digit() || ((*c == ',' || *c == '.') && next_is_digit) {
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

/// The value of a written number and its decimals; thousands separators are dropped.
fn parse(written: &str) -> Option<(f64, u32)> {
    let plain: String = written.chars().filter(|c| *c != ',').collect();
    let decimals = plain
        .split_once('.')
        .map_or(0, |(_, d)| u32::try_from(d.len()).unwrap_or(u32::MAX));
    plain.parse::<f64>().ok().map(|v| (v, decimals))
}

/// `fact` written with `decimals` decimals reads as `value`.
fn rounds_to(fact: f64, value: f64, decimals: u32) -> bool {
    let Ok(d) = i32::try_from(decimals.min(9)) else {
        return false;
    };
    let scale = 10f64.powi(d);
    ((fact.abs() * scale).round() - (value * scale).round()).abs() < 0.5
}

/// Numbers in `texts` that appear nowhere in `facts` (signs ignored: "848 kg less" quotes
/// -848), in order of appearance, each once.
pub fn unsupported_numbers(texts: &[&str], facts: &str) -> Vec<String> {
    let known: Vec<f64> = numbers(facts)
        .iter()
        .filter_map(|n| parse(n))
        .map(|(v, _)| v)
        .collect();
    let mut bad: Vec<String> = Vec::new();
    for text in texts {
        for written in numbers(text) {
            let Some((value, decimals)) = parse(&written) else {
                continue;
            };
            let small = decimals == 0 && value <= 12.0;
            let quoted = known.iter().any(|f| rounds_to(*f, value, decimals));
            if !small && !quoted && !bad.contains(&written) {
                bad.push(written);
            }
        }
    }
    bad
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_as_written() {
        assert_eq!(
            numbers("1,644 kg, then 0.119 at day 186 00:40; floor1_reaAHU and 3."),
            ["1,644", "0.119", "186", "00", "40", "1", "3"]
        );
        assert_eq!(parse("1,644"), Some((1644.0, 0)));
        assert_eq!(parse("0.12"), Some((0.12, 2)));
    }

    #[test]
    fn quoted_and_small_numbers_pass_and_invented_ones_do_not() {
        let facts = r#"{"scope1_kg_co2e": 1644, "change_kg_co2e": -848, "tdis": 0.119, "from": "day 143 06:00", "steps": 240}"#;
        let ok = "Gas fell by 848 kg to 1,644 kg on day 143 from 06:00 over 240 steps, about 8 hours; \
                  discomfort was 0.12.";
        assert!(unsupported_numbers(&[ok], facts).is_empty());
        let invented = "That is a 52% cut, or 1,234 kWh.";
        assert_eq!(
            unsupported_numbers(&["fine", invented], facts),
            ["52", "1,234"]
        );
        assert_eq!(unsupported_numbers(&["0.5 K warmer"], facts), ["0.5"]);
    }
}
