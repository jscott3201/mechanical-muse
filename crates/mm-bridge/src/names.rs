//! Matching a short point name (`yFault`) to a canonical engine path
//! (`urn:...#rule.yFault`): an exact match first, else exactly one `.name` or `#name` suffix.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("no point named {0}")]
    Unknown(String),
    #[error("{name} matches several points: {matches:?}")]
    Ambiguous { name: String, matches: Vec<String> },
}

pub fn resolve_name<'a>(name: &str, paths: &'a [String]) -> Result<&'a str, NameError> {
    if let Some(exact) = paths.iter().find(|p| p.as_str() == name) {
        return Ok(exact);
    }
    let dotted = format!(".{name}");
    let hashed = format!("#{name}");
    let matches: Vec<&String> = paths
        .iter()
        .filter(|p| p.ends_with(&dotted) || p.ends_with(&hashed))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.as_str()),
        [] => Err(NameError::Unknown(name.to_owned())),
        many => Err(NameError::Ambiguous {
            name: name.to_owned(),
            matches: many.iter().map(|m| (*m).clone()).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        ["urn:x#r.mat", "urn:x#r.sat", "urn:x#r.gap.u1", "urn:x#u1"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn resolves_exact_then_suffix() {
        let p = paths();
        assert_eq!(resolve_name("urn:x#r.mat", &p).unwrap(), "urn:x#r.mat");
        assert_eq!(resolve_name("sat", &p).unwrap(), "urn:x#r.sat");
    }

    #[test]
    fn rejects_unknown_and_ambiguous() {
        let p = paths();
        assert_eq!(
            resolve_name("oat", &p),
            Err(NameError::Unknown("oat".into()))
        );
        assert!(matches!(
            resolve_name("u1", &p),
            Err(NameError::Ambiguous { .. })
        ));
        // A partial word is not a suffix match.
        assert!(resolve_name("at", &p).is_err());
    }
}
