//! Mermaid state diagram parser for FSM definitions.
//!
//! Parses Mermaid stateDiagram syntax to extract states, transitions, and events.
//!
//! Supported syntax:
//! - `state1 --> |event| state2` - transition with event
//! - `[*] --> state` - initial state (start)
//! - `state --> [*]` - terminal state (end)
//! - `state --> |after(duration)| next` - timeout event

use std::collections::{HashMap, HashSet};
use syn::{Error, LitStr, Result};

/// Represents a parsed FSM definition
#[derive(Debug, Clone)]
pub struct Fsm {
    /// All states in the FSM
    pub states: HashSet<String>,
    /// Initial state (from [*] --> state)
    pub initial_state: Option<String>,
    /// Terminal states (state --> [*])
    pub terminal_states: HashSet<String>,
    /// Transitions: (from_state, event) -> to_state
    pub transitions: HashMap<(String, String), String>,
    /// All events in the FSM
    pub events: HashSet<String>,
}

impl Fsm {
    /// Parse Mermaid state diagram from a string literal
    pub fn parse(lit: &LitStr) -> Result<Self> {
        let content = lit.value();
        let mut fsm = Fsm {
            states: HashSet::new(),
            initial_state: None,
            terminal_states: HashSet::new(),
            transitions: HashMap::new(),
            events: HashSet::new(),
        };

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("%%") {
                continue;
            }

            // Skip stateDiagram-v2 declaration
            if line.starts_with("stateDiagram") {
                continue;
            }

            // Parse transition: state1 --> |event| state2
            if let Some((from, rest)) = line.split_once("-->") {
                let from = from.trim();
                let rest = rest.trim();

                // Extract event and target state
                let (event, to) = if let Some(pipe_start) = rest.find('|') {
                    if let Some(pipe_end) = rest[pipe_start + 1..].find('|') {
                        let event = rest[pipe_start + 1..pipe_start + 1 + pipe_end].trim();
                        let to = rest[pipe_start + 1 + pipe_end + 1..].trim();
                        (Some(event), to)
                    } else {
                        return Err(Error::new(
                            lit.span(),
                            format!("Invalid transition syntax: {}", line),
                        ));
                    }
                } else {
                    // No event specified
                    (None, rest)
                };

                // Handle special states
                match (from, to) {
                    ("[*]", to) if to != "[*]" => {
                        // Initial state
                        fsm.initial_state = Some(to.to_string());
                        fsm.states.insert(to.to_string());
                    }
                    (from, "[*]") if from != "[*]" => {
                        // Terminal state
                        fsm.states.insert(from.to_string());
                        fsm.terminal_states.insert(from.to_string());

                        if let Some(event) = event {
                            fsm.events.insert(event.to_string());
                            fsm.transitions
                                .insert((from.to_string(), event.to_string()), "[*]".to_string());
                        }
                    }
                    (from, to) if from != "[*]" && to != "[*]" => {
                        // Regular transition
                        fsm.states.insert(from.to_string());
                        fsm.states.insert(to.to_string());

                        if let Some(event) = event {
                            fsm.events.insert(event.to_string());
                            fsm.transitions
                                .insert((from.to_string(), event.to_string()), to.to_string());
                        }
                    }
                    _ => {
                        return Err(Error::new(
                            lit.span(),
                            format!("Invalid transition: {}", line),
                        ));
                    }
                }
            }
        }

        // Validate FSM
        if fsm.states.is_empty() {
            return Err(Error::new(lit.span(), "FSM has no states"));
        }

        if fsm.initial_state.is_none() {
            return Err(Error::new(
                lit.span(),
                "FSM has no initial state (use [*] --> state)",
            ));
        }

        Ok(fsm)
    }

    /// Check if a state is terminal (has no outgoing transitions)
    #[allow(dead_code)]
    pub fn is_terminal(&self, state: &str) -> bool {
        self.terminal_states.contains(state)
            || !self.transitions.keys().any(|(from, _)| {
                from == state && self.transitions[&(from.clone(), "".to_string())] != "[*]"
            })
    }

    /// Get all possible transitions from a state
    #[allow(dead_code)]
    pub fn transitions_from(&self, state: &str) -> Vec<(&String, &String)> {
        self.transitions
            .iter()
            .filter_map(|((from, event), to)| {
                if from == state {
                    Some((event, to))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Validate a transition is allowed
    #[allow(dead_code)]
    pub fn is_valid_transition(&self, from: &str, event: &str, to: &str) -> bool {
        self.transitions
            .get(&(from.to_string(), event.to_string()))
            .map(|expected_to| expected_to == to || expected_to == "[*]")
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_parse_simple_fsm() {
        let lit: LitStr = parse_quote! {
            r#"
            [*] --> locked
            locked --> |coin| unlocked
            unlocked --> |push| locked
            "#
        };

        let fsm = Fsm::parse(&lit).unwrap();

        assert_eq!(fsm.initial_state, Some("locked".to_string()));
        assert_eq!(fsm.states.len(), 2);
        assert!(fsm.states.contains("locked"));
        assert!(fsm.states.contains("unlocked"));
        assert_eq!(fsm.events.len(), 2);
        assert!(fsm.events.contains("coin"));
        assert!(fsm.events.contains("push"));
    }

    #[test]
    fn test_parse_terminal_state() {
        let lit: LitStr = parse_quote! {
            r#"
            [*] --> active
            active --> |stop| [*]
            "#
        };

        let fsm = Fsm::parse(&lit).unwrap();

        assert!(fsm.terminal_states.contains("active"));
    }
}
