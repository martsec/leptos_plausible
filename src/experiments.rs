//! Support for using AB tests (experiments) where the system randomly selects
//! a given Variant to show the user.
//!
//!
//!
use std::collections::HashMap;

use rand::distributions::WeightedIndex;
use rand::prelude::*;

use leptos::{
    component, create_resource, store_value, use_context, view, ChildrenFn, IntoView, Provider,
    Resource, Show, SignalGet, Suspense,
};
use serde::{Deserialize, Serialize};

use crate::components::TrackElement;
use crate::event::PropValue;

/// Represents the variant of an experiment with its custom name and weight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    pub weight: u16,
}

impl Variant {
    #[must_use]
    pub fn new(name: &str, weight: u16) -> Self {
        Self {
            name: name.into(),
            weight,
        }
    }
}

/// Define a new weighted A/B test
///
/// This class is used to define the odds and pass it around
/// as context so it gets propagated from (ideally) the top of the
/// structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub name: String,
    a: Variant,
    b: Variant,
    /// Selected variant
    pub selected: usize,
}

impl Experiment {
    #[must_use]
    pub fn new(name: &str, a: Variant, b: Variant) -> Self {
        Self {
            a,
            b,
            name: name.into(),
            selected: 0,
        }
    }

    /// Choose the variant to show given the weights.
    ///
    /// It's separated from the init since this needs to happen in a leptos' [`local_resource`]
    /// to avoud hydration bugs
    pub fn choose(&mut self) {
        // Using RNG in SSR will cause hydration bugs unless it's within a `local_resource`
        let weights: Vec<u16> = vec![self.a.weight, self.b.weight];
        let dist = WeightedIndex::new(weights).expect("ERR in experiment");
        let mut rng = thread_rng();
        let res = dist.sample(&mut rng);
        self.selected = res;
    }

    /// Returns the choosen variant
    #[must_use]
    pub const fn variant(&self) -> &Variant {
        match self.selected {
            0 => &self.a,
            _ => &self.b,
        }
    }
}

impl Default for Experiment {
    /// Inits a default experiment with A and B variants with the same weight.
    fn default() -> Self {
        Self {
            name: String::from("Experiment"),
            a: Variant::new("A", 1),
            b: Variant::new("B", 1),
            selected: 0,
        }
    }
}

/// A component that will show the choosen variant of the experiment.
///
/// It will always send an event called `ExperimentView` when any of the
/// variants appears in the viewport (see
/// [`leptos_use::use_element_visibility`](https://leptos-use.rs/elements/use_element_visibility.html))
/// and will provide the experiment as context to be used by the downstream events.
///
/// ```rust
/// # use leptos::*;
/// # use plaicards::web::plausible::experiments::{Variant, Experiment, ExperimentView};
/// # let runtime = create_runtime();
/// # // create_runtime fails when trying to use create_resource
/// # if false {
///
/// #[component]
/// fn ComponentA() -> impl IntoView {
///     view! { <div> "I'm A" </div>}
/// }
/// #[component]
/// fn ComponentB() -> impl IntoView {
///     view! { <div> "I'm B" </div>}
/// }
///
/// let e = Experiment::new(
///     "Experiment",
///     Variant::new("A", 1),
///     Variant::new("B", 9)
/// );
///
/// view! {
///   <ExperimentView
///     exp=e
///     a=ComponentA
///   >
///     <ComponentB />
///   </ExperimentView>
/// }
/// # ;
/// # }
/// # runtime.dispose();
/// ```
#[must_use]
#[component]
pub fn ExperimentView<F, IV>(
    exp: Experiment,
    //#[prop(into)] a: F,
    //#[prop(into)] b: F,
    a: F,
    children: ChildrenFn,
) -> impl IntoView
where
    F: Fn() -> IV + 'static,
    IV: IntoView,
{
    let exp = store_value(exp);
    let variant = create_resource(
        || (),
        move |()| async move {
            let mut e = exp.get_value();
            e.choose();
            e
        },
    );

    // Store the views so we can "Copy" its references within other components
    let a = store_value(a);
    let b = store_value(children);

    view! {
        <Provider value=ExperimentCtx(variant)>
            // This provides the value ONLY to its children
            // see https://github.com/leptos-rs/book/issues/3
            <Suspense fallback=|| ()>
                <TrackElement name="ExperimentView"/>
                <Show
                    when=move || variant.get().map_or_else(|| false, |v| v.selected == 1)
                    fallback=move || a.with_value(|a| a())
                >
                    {b.with_value(|b| b())}
                </Show>
            </Suspense>
        </Provider>
    }
}

/// Retrieve experiment from leptos context.
///
#[must_use]
pub fn use_experiment() -> Option<ExperimentCtx> {
    use_context::<ExperimentCtx>()
}

#[must_use]
pub fn use_experiment_props() -> Option<HashMap<String, PropValue>> {
    use_experiment().map(|e| e.to_plausible())
}

#[derive(Copy, Clone, Debug)]
pub struct ExperimentCtx(pub Resource<(), Experiment>);

impl ExperimentCtx {
    pub fn to_plausible(&self) -> HashMap<String, PropValue> {
        self.0.get().map_or_else(HashMap::new, |e| {
            HashMap::from([(format!("exp_{}", e.name), e.variant().name.clone().into())])
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[component]
    fn ComponentA() -> impl IntoView {
        view! { <div>"I'm A"</div> }
    }
    #[component]
    fn ComponentB() -> impl IntoView {
        view! { <div>"I'm B"</div> }
    }
    #[component]
    fn ComponentC() -> impl IntoView {
        view! { <div>"I'm C"</div> }
    }

    #[test]
    fn build_variant() {
        Variant {
            name: String::from("A"),
            weight: 1,
        };
    }

    #[test]
    fn weighted_experiments() {
        let mut e = Experiment::new("Experiment", Variant::new("A", 1), Variant::new("B", 9));

        let choices: Vec<usize> = (0..1000)
            .map(|_| {
                e.choose();
                e.selected
            })
            .collect();

        let a_count = choices.iter().filter(|v| **v == 0).count();
        let b_count = choices.iter().filter(|v| **v == 1).count();

        println!("{} {}", a_count, b_count);
        assert!(
            50 <= a_count && a_count <= 150,
            "Weights do not seem to work"
        );
        assert!(
            800 <= b_count && b_count <= 980,
            "Weights do not seem to work"
        );
    }
}
