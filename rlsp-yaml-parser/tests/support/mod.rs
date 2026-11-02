// SPDX-License-Identifier: MIT
//
// Helpers that more than one test crate of rlsp-yaml-parser uses.  Imported via
// `mod support;`.

use rlsp_yaml_parser::Event;

/// `Ok` when the collections in `events` nest properly and every mapping holds
/// an even number of nodes, that is each key has a value; otherwise what is
/// wrong.  A collection counts as one node of the collection that holds it.
pub fn check_well_formed<'a>(events: impl IntoIterator<Item = Event<'a>>) -> Result<(), String> {
    // One counter per open collection: (is a mapping, nodes seen so far).
    let mut open: Vec<(bool, usize)> = Vec::new();
    let complete_node = |open: &mut Vec<(bool, usize)>| {
        if let Some((_, nodes)) = open.last_mut() {
            *nodes += 1;
        }
    };
    for event in events {
        match event {
            Event::MappingStart { .. } => open.push((true, 0)),
            Event::SequenceStart { .. } => open.push((false, 0)),
            Event::MappingEnd => {
                let Some((is_mapping, nodes)) = open.pop() else {
                    return Err("a mapping closes that was never opened".to_owned());
                };
                if !is_mapping {
                    return Err("a mapping closes a sequence".to_owned());
                }
                if nodes % 2 != 0 {
                    return Err(format!("mapping with {nodes} nodes"));
                }
                complete_node(&mut open);
            }
            Event::SequenceEnd => {
                let Some((is_mapping, _)) = open.pop() else {
                    return Err("a sequence closes that was never opened".to_owned());
                };
                if is_mapping {
                    return Err("a sequence closes a mapping".to_owned());
                }
                complete_node(&mut open);
            }
            Event::Scalar { .. } | Event::Alias { .. } => complete_node(&mut open),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. } => {}
        }
    }
    if open.is_empty() {
        Ok(())
    } else {
        Err("collections left open".to_owned())
    }
}
