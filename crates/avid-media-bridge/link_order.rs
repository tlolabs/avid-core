use std::collections::HashSet;

/// pkg-config returns dependents before providers. Keep the final occurrence of
/// each library across its complete dependency closures, so a later dependent
/// cannot end up behind a provider that was first seen in an earlier closure.
/// This order works with single-pass archive linkers without whole-archive or
/// start-group flags, and avoids duplicating object members in Rust staticlibs.
pub fn libraries_in_link_order(groups: &[Vec<String>]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result: Vec<_> = groups
        .iter()
        .flat_map(|group| group.iter())
        .rev()
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect();
    result.reverse();
    result
}
