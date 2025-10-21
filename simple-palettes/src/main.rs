#![feature(more_qualified_paths)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(clippy::exhaustive_enums, reason = "conflicts with dioxus")]
#![allow(clippy::same_name_method, reason = "conflicts with dioxus")]
#![allow(clippy::impl_trait_in_params, reason = "conflicts with dioxus")]
#![allow(missing_docs, reason = "todo: better documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "TODO: documentation")]
#![allow(clippy::missing_errors_doc, reason = "TODO: documentation")]

mod components;

use dioxus::prelude::*;
use gloo_storage::errors::StorageError;
use gloo_storage::{LocalStorage, Storage as _};
use serde::Deserialize;

#[expect(dead_code, reason = "not used yet")]
const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
#[expect(dead_code, reason = "not used yet")]
const HEADER_SVG: Asset = asset!("/assets/header.svg");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[non_exhaustive]
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    DogView,
}

#[expect(
    clippy::unused_async,
    reason = "false positive (`server` attribute adds `await`)"
)]
#[server(endpoint = "static_routes", output = server_fn::codec::Json)]
async fn static_routes() -> Result<Vec<String>, ServerFnError> {
    Ok(Route::static_routes()
        .iter()
        .map(ToString::to_string)
        .collect())
}

#[expect(unused, reason = "false positive (used by `Title`)")]
#[derive(Clone)]
struct TitleState(String);

#[component]
fn Title() -> Element {
    let title = use_context::<TitleState>();

    rsx! {
        div { id: "title",
            h1 { "{title.0}" }
        }
    }
}

#[derive(Deserialize)]
struct DogApi {
    message: String,
}

fn save_dog(image: String) -> Result<(), StorageError> {
    let mut saved: Vec<String> = LocalStorage::get("saved_dogs").unwrap_or_default();
    saved.push(image);

    LocalStorage::set("saved_dogs", saved)
}

#[expect(
    unused_qualifications,
    reason = "it thinks `onclick` is a path segment"
)]
#[component]
fn DogView() -> Element {
    let mut img_src = use_resource(async || {
        reqwest::get("https://dog.ceo/api/breeds/image/random")
            .await
            .unwrap()
            .json::<DogApi>()
            .await
            .unwrap()
            .message
    });

    rsx! {
        div { id: "dogview",
            img { src: img_src.cloned().unwrap_or_default() }
        }
        div { id: "buttons",
            button { id: "skip", onclick: move |_| img_src.restart(), "skip", }

            button { id: "save",
                onclick: move |_| {
                    if let Some(current) = img_src.cloned() {
                        img_src.restart();
                        let _result = save_dog(current);
                    }
                },

                "save!",
            }
        }
    }
}

#[component]
fn App() -> Element {
    rsx! {
        Stylesheet { href: TAILWIND_CSS }
        Stylesheet { href: MAIN_CSS }

        Router::<Route> {}
    }
}

fn main() {
    LaunchBuilder::new()
        .with_cfg(server_only! {
            ServeConfig::builder()
                .incremental(
                    dioxus_server::IncrementalRendererConfig::new()
                        .static_dir(
                            std::env::current_exe()
                                .unwrap()
                                .parent()
                                .unwrap()
                                .join("public")
                            )
                            .clear_cache(false)
                        )
                        .enable_out_of_order_streaming()
        })
        .launch(App);
}
