//! Procedural macros for joerl
//!
//! This crate provides compile-time code generation for FSM definitions using Mermaid syntax.

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Meta, parse_macro_input};

mod mermaid;

use mermaid::Fsm;

/// Generate a GenStatem implementation from a Mermaid state diagram.
///
/// This macro parses a Mermaid stateDiagram and generates the boilerplate
/// Actor implementation, calling user-defined callbacks for transitions.
///
/// # Example
///
/// ```ignore
/// use joerl_macro::gen_statem;
///
/// #[gen_statem(fsm = r#"
///     [*] --> locked
///     locked --> |coin| unlocked
///     locked --> |push| locked
///     unlocked --> |push| locked
///     unlocked --> |coin| unlocked
///     unlocked --> |off| [*]
/// "#)]
/// struct Turnstile {
///     donations: u32,
/// }
///
/// impl Turnstile {
///     fn on_transition(&mut self, event: TurnstileEvent, state: TurnstileState, data: &mut TurnstileData) -> TransitionResult {
///         match (state, event) {
///             (TurnstileState::Locked, TurnstileEvent::Coin) => {
///                 TransitionResult::next(TurnstileState::Unlocked, data.clone())
///             }
///             // ... other transitions
///         }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn gen_statem(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    // Parse attribute to extract FSM definition
    let fsm_str = if attr.is_empty() {
        return syn::Error::new_spanned(
            &input,
            "gen_statem requires an fsm parameter: #[gen_statem(fsm = \"...\")]",
        )
        .to_compile_error()
        .into();
    } else {
        // Parse attribute as Meta
        let meta = match syn::parse::<Meta>(attr.clone()) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };

        // Extract fsm = "..." value
        match meta {
            Meta::NameValue(nv) if nv.path.is_ident("fsm") => match nv.value {
                syn::Expr::Lit(ref lit) => {
                    if let syn::Lit::Str(ref s) = lit.lit {
                        s.clone()
                    } else {
                        return syn::Error::new_spanned(
                            &nv.value,
                            "fsm value must be a string literal",
                        )
                        .to_compile_error()
                        .into();
                    }
                }
                _ => {
                    return syn::Error::new_spanned(
                        &nv.value,
                        "fsm value must be a string literal",
                    )
                    .to_compile_error()
                    .into();
                }
            },
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "gen_statem requires: #[gen_statem(fsm = \"...\")]",
                )
                .to_compile_error()
                .into();
            }
        }
    };

    // Parse FSM from Mermaid
    let fsm = match Fsm::parse(&fsm_str) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    // Generate code
    match generate_gen_statem(&input, &fsm) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate_gen_statem(input: &DeriveInput, fsm: &Fsm) -> syn::Result<proc_macro2::TokenStream> {
    let struct_name = &input.ident;
    let vis = &input.vis;

    // Generate state enum
    let state_enum_name = syn::Ident::new(&format!("{}State", struct_name), struct_name.span());
    let state_variants: Vec<_> = fsm
        .states
        .iter()
        .map(|s| {
            let variant = to_pascal_case(s);
            syn::Ident::new(&variant, struct_name.span())
        })
        .collect();

    // Generate event enum
    let event_enum_name = syn::Ident::new(&format!("{}Event", struct_name), struct_name.span());
    let event_variants: Vec<_> = fsm
        .events
        .iter()
        .map(|e| {
            let variant = to_pascal_case(e);
            syn::Ident::new(&variant, struct_name.span())
        })
        .collect();

    // Data type is the struct itself
    let data_type_name = syn::Ident::new(&format!("{}Data", struct_name), struct_name.span());

    // Generate transition validation
    let transition_checks: Vec<_> = fsm
        .transitions
        .iter()
        .map(|((from, event), to)| {
            let from_var = syn::Ident::new(&to_pascal_case(from), struct_name.span());
            let event_var = syn::Ident::new(&to_pascal_case(event), struct_name.span());
            let to_var = if to == "[*]" {
                let to_ident = syn::Ident::new(&to_pascal_case(from), struct_name.span());
                quote! { Some(#state_enum_name::#to_ident) } // Terminal transitions stay in same state before stopping
            } else {
                let to_ident = syn::Ident::new(&to_pascal_case(to), struct_name.span());
                quote! { Some(#state_enum_name::#to_ident) }
            };

            quote! {
                (#state_enum_name::#from_var, #event_enum_name::#event_var) => #to_var,
            }
        })
        .collect();

    // Generate terminal transition checks - (state, event) pairs that lead to [*]
    let terminal_transition_checks: Vec<_> = fsm
        .transitions
        .iter()
        .filter_map(|((from, event), to)| {
            if to == "[*]" {
                let from_var = syn::Ident::new(&to_pascal_case(from), struct_name.span());
                let event_var = syn::Ident::new(&to_pascal_case(event), struct_name.span());
                Some(quote! {
                    (#state_enum_name::#from_var, #event_enum_name::#event_var)
                })
            } else {
                None
            }
        })
        .collect();

    let initial_state = fsm
        .initial_state
        .as_ref()
        .expect("FSM must have initial state");
    let initial_state_var = syn::Ident::new(&to_pascal_case(initial_state), struct_name.span());

    let actor_struct_name = syn::Ident::new(&format!("{}Actor", struct_name), struct_name.span());

    // Generate the complete implementation
    let generated = quote! {
        // Original struct definition
        #input

        // State enum
        #[derive(Debug, Clone, PartialEq, Eq)]
        #vis enum #state_enum_name {
            #(#state_variants),*
        }

        // Event enum
        #[derive(Debug, Clone)]
        #vis enum #event_enum_name {
            #(#event_variants),*
        }

        // Data type (alias to original struct)
        #vis type #data_type_name = #struct_name;

        // Transition result
        #[derive(Debug)]
        #vis enum TransitionResult {
            Next(#state_enum_name, #data_type_name),
            Keep(#data_type_name),
            Error(String),
        }

        // Internal actor implementation
        struct #actor_struct_name {
            state: Option<#state_enum_name>,
            data: Option<#data_type_name>,
        }

        impl #actor_struct_name {
            fn new(data: #data_type_name) -> Self {
                Self {
                    state: Some(#state_enum_name::#initial_state_var),
                    data: Some(data),
                }
            }
        }

        #[joerl::async_trait::async_trait]
        impl joerl::Actor for #actor_struct_name {
            async fn handle_message(&mut self, msg: joerl::Message, ctx: &mut joerl::ActorContext) {
                let state = match self.state.take() {
                    Some(s) => s,
                    None => return,
                };

                let mut data = match self.data.take() {
                    Some(d) => d,
                    None => {
                        self.state = Some(state);
                        return;
                    }
                };

                // Downcast to event
                if let Ok(event) = msg.downcast::<#event_enum_name>() {
                    let event = *event; // Unbox the event
                    // Call user callback on_transition (passing event, state by value)
                    let result = data.on_transition(event.clone(), state.clone());

                    match result {
                        TransitionResult::Next(new_state, new_data) => {
                            // Validate transition
                            let expected = match (&state, &event) {
                                #(#transition_checks)*
                                _ => None,
                            };

                            if let Some(expected_state) = expected {
                                if expected_state != new_state {
                                    tracing::warn!(
                                        "Transition validation failed: expected {:?}, got {:?}",
                                        expected_state, new_state
                                    );
                                    self.state = Some(state);
                                    self.data = Some(data);
                                    return;
                                }
                            } else {
                                tracing::warn!("Invalid transition from {:?} with event {:?}", state, event);
                                self.state = Some(state);
                                self.data = Some(data);
                                return;
                            }

                            // Call on_enter callback if it exists
                            new_data.on_enter(&state, &new_state, &new_data);

                            // Check if this was a terminal transition (to [*])
                            let is_terminal = matches!((&state, &event), #(#terminal_transition_checks)|*);

                            if is_terminal {
                                // Call on_terminate before stopping
                                new_data.on_terminate(&joerl::ExitReason::Normal, &new_state, &new_data);
                                ctx.stop(joerl::ExitReason::Normal);
                            }

                            self.state = Some(new_state);
                            self.data = Some(new_data);
                        }
                        TransitionResult::Keep(kept_data) => {
                            // Check if this was a terminal transition even with Keep
                            let is_terminal = matches!((&state, &event), #(#terminal_transition_checks)|*);

                            if is_terminal {
                                // Call on_terminate before stopping
                                kept_data.on_terminate(&joerl::ExitReason::Normal, &state, &kept_data);
                                ctx.stop(joerl::ExitReason::Normal);
                            }

                            self.state = Some(state);
                            self.data = Some(kept_data);
                        }
                        TransitionResult::Error(_err) => {
                            // Keep current state on error
                            self.state = Some(state);
                            self.data = Some(data);
                        }
                    }
                } else {
                    self.state = Some(state);
                    self.data = Some(data);
                }
            }

            async fn stopped(&mut self, reason: &joerl::ExitReason, _ctx: &mut joerl::ActorContext) {
                if let (Some(state), Some(data)) = (self.state.as_ref(), self.data.as_mut()) {
                    data.on_terminate(reason, state, data);
                }
            }
        }

        // Spawn helper function
        #[allow(non_snake_case)]
        #vis fn #struct_name(system: &std::sync::Arc<joerl::ActorSystem>, data: #data_type_name) -> joerl::ActorRef {
            let actor = #actor_struct_name::new(data);
            system.spawn(actor)
        }
    };

    Ok(generated)
}

/// Convert snake_case or kebab-case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', '!', '?'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("locked"), "Locked");
        assert_eq!(to_pascal_case("un_locked"), "UnLocked");
        assert_eq!(to_pascal_case("on!"), "On");
        assert_eq!(to_pascal_case("coin?"), "Coin");
    }
}
