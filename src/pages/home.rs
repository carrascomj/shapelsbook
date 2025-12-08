use crate::components::counter_btn::Button;
use leptos::prelude::*;
use shapels::analyze_source;

fn analyze_diagnostics(src: &str) -> String {
    // Your real analysis here
    let analysis = analyze_source(src);
    analysis.diagnostics.iter().map(|x| format!("{x:#?}")).collect()
}


/// Code text prefilled with code, the user can modify it.
///
/// On change, it reruns the language serve, updates the hovers
/// and the diagnostics.
#[component]
fn CodeInput() -> impl IntoView {
    let initial_code = r#"
import jaxtyping
import torch
    
def proper_multiply_bad_annotation_should_produce_diagnostics(x: jaxtyping.Float[torch.Tensor, "B X R"], y: jaxtyping.Float[torch.Tensor, "R S"]) -> jaxtyping.Float[torch.Tensor, "B S"]:
    z: jaxtyping.Float[torch.Tensor, "B X R"] = x @ y
    return z 
"#;

    let (code, set_code) = signal(initial_code.to_string());

    // Recompute whenever `code` changes
    let analysis = Memo::new(move |_| analyze_diagnostics(&code.get()));

    view! {
        <textarea
            // update the signal on each keystroke
            bind:value=(code, set_code)
        ></textarea>

        <p>
            {move || analysis.get()}
        </p>
    }
}
/// Default Home Page
#[component]
pub fn Home() -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            view! {
                <h1>"Uh oh! Something went wrong!"</h1>

                <p>"Errors: "</p>
                // Render a list of errors as strings - good for development purposes
                <ul>
                    {move || {
                        errors
                            .get()
                            .into_iter()
                            .map(|(_, e)| view! { <li>{e.to_string()}</li> })
                            .collect_view()
                    }}

                </ul>
            }
        }>

            <div class="container">

                // <picture>
                //     <source
                //         srcset="https://raw.githubusercontent.com/leptos-rs/leptos/main/docs/logos/Leptos_logo_pref_dark_RGB.svg"
                //         media="(prefers-color-scheme: dark)"
                //     />
                //     <img
                //         src="https://raw.githubusercontent.com/leptos-rs/leptos/main/docs/logos/Leptos_logo_RGB.svg"
                //         alt="Leptos Logo"
                //         height="200"
                //         width="400"
                //     />
                // </picture>

                <h1>"shapels: a primer"</h1>

                <CodeInput />

            </div>
        </ErrorBoundary>
    }
}
