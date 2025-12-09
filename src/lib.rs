use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

// Modules
mod pages;

// Top-Level pages
use crate::pages::home::Home;

/// An app router which renders the homepage and handles 404's
#[component]
pub fn App() -> impl IntoView {
    // Base path for deployment on GitHub Pages; defaults to repo name when not in debug.
    let base = if cfg!(debug_assertions) {
        "/"
    } else {
        "/shapelsbook/"
    };

    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <Html attr:lang="en" attr:dir="ltr" attr:data-theme="light" />

        // sets the document title
        <Title text="The shapels book" />

        // injects metadata in the <head> of the page
        <Meta charset="UTF-8" />
        <Meta name="viewport" content="width=device-width, initial-scale=1.0" />

        <Router>
            <Router base=base>
                <Routes fallback=|| view! { NotFound }>
                    <Route path=path!("/") view=Home />
                </Routes>
            </Router>
        </Router>
    }
}
