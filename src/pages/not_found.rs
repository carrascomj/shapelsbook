use leptos::ev::MouseEvent;
use leptos::prelude::window;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::app_path::app_base_path;

/// 404 Not Found Page
#[component]
pub fn NotFound() -> impl IntoView {
    let location = use_location();
    let route_literal = move || format!("\"{}\"", location.pathname.get());
    let home_href = Memo::new(move |_| {
        let origin = window().location().origin().unwrap_or_default();
        format!("{origin}{}/", app_base_path())
    });
    let route_hover = RwSignal::new(None::<(f64, f64)>);
    let show_route_hover = move |ev: MouseEvent| {
        route_hover.set(Some((
            ev.client_x() as f64 + 14.0,
            ev.client_y() as f64 + 18.0,
        )));
    };
    let hide_route_hover = move |_| {
        route_hover.set(None);
    };

    view! {
        <section class="not-found-stage">
            <div class="not-found-shell">
                <div class="not-found-editor">
                    <div class="not-found-watermark">"404"</div>

                    <div class="not-found-row not-found-row--source">
                        <span class="not-found-bullet">"●"</span>
                        <span class="not-found-line-no">"1"</span>
                        <span class="not-found-code">
                            <span
                                class="tok-ident tok-ident--interactive"
                                on:mouseenter=show_route_hover
                                on:mousemove=show_route_hover
                                on:mouseleave=hide_route_hover
                            >
                                "route"
                            </span>
                            <span class="tok-operator">" = "</span>
                            <span class="tok-string">{route_literal}</span>
                        </span>
                    </div>

                    <div class="not-found-diagnostics">
                        <div class="not-found-row not-found-row--diag">
                            <span class="not-found-tree not-found-tree--branch">"├─"</span>
                            <span class="diag-tag diag-tag--error">"ERROR"</span>
                            <span class="diag-message diag-message--error">"Page not found"</span>
                            <span class="diag-code">"[404]"</span>
                        </div>

                        <div class="not-found-row not-found-row--diag">
                            <span class="not-found-tree not-found-tree--end">"└─"</span>
                            <span class="diag-tag diag-tag--hint">"HINT"</span>
                            <span class="diag-message diag-message--hint">
                                <a href=move || home_href.get() class="not-found-inline-link">
                                    "Click here"
                                </a>
                                " to go back to shapels' homepage"
                            </span>
                        </div>
                    </div>
                </div>
            </div>

            {move || {
                route_hover.get().map(|(x, y)| {
                    view! {
                        <div
                            class="not-found-hover-tooltip"
                            style=format!("left: {:.2}px; top: {:.2}px;", x, y)
                        >
                            "Maybe try \"/playground\" instead"
                        </div>
                    }
                })
            }}
        </section>
    }
}
