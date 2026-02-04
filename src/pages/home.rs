use leptos::ev::MouseEvent;
use leptos::prelude::window;
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys::Element;
use lsp_types::{Diagnostic, Position};
use shapels::analyze_source;
use std::collections::HashSet;
use std::sync::Arc;

fn render_hover_text(info: &shapels::HoverInfo) -> String {
    if let Some(shape) = &info.shape {
        format!(
            "{}: {}",
            shape.render(),
            shape.dtype.as_deref().unwrap_or("Any")
        )
    } else {
        String::from("hover unavailable")
    }
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "and"
            | "as"
            | "assert"
            | "break"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "False"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "None"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "True"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn highlight_classes(line: &str) -> Vec<Option<&'static str>> {
    let chars: Vec<char> = line.chars().collect();
    let mut classes = vec![None; chars.len()];
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let quote = ch;
            classes[i] = Some("hl-string");
            i += 1;
            while i < chars.len() {
                classes[i] = Some("hl-string");
                let cur = chars[i];
                if cur == quote && (i == 0 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if is_keyword(&token) {
                for idx in start..i {
                    classes[idx] = Some("hl-keyword");
                }
            }
            continue;
        }

        if ch == '#' {
            for idx in i..chars.len() {
                classes[idx] = Some("hl-comment");
            }
            break;
        }

        i += 1;
    }

    classes
}

fn diag_mask_for_line(line_index: u32, line_len: u32, diagnostics: &[Diagnostic]) -> Vec<bool> {
    let mut mask = vec![false; line_len as usize];
    for diag in diagnostics {
        let range = &diag.range;
        if line_index < range.start.line || line_index > range.end.line {
            continue;
        }
        let start_char = if line_index == range.start.line {
            range.start.character
        } else {
            0
        };
        let end_char = if line_index == range.end.line {
            range.end.character
        } else {
            line_len
        };
        let start_char = start_char.min(line_len);
        let end_char = end_char.min(line_len);
        if end_char <= start_char {
            continue;
        }
        for idx in start_char..end_char {
            if let Some(slot) = mask.get_mut(idx as usize) {
                *slot = true;
            }
        }
    }
    mask
}

fn build_line_spans(
    line: &str,
    line_index: u32,
    diagnostics: &[Diagnostic],
) -> Vec<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![(" ".to_string(), String::new())];
    }

    let highlight = highlight_classes(line);
    let diag_mask = diag_mask_for_line(line_index, chars.len() as u32, diagnostics);

    let mut spans = Vec::new();
    let mut current_class = String::new();
    let mut current_text = String::new();

    for (idx, ch) in chars.iter().enumerate() {
        let mut class = String::new();
        if let Some(Some(hl)) = highlight.get(idx) {
            class.push_str(hl);
        }
        if diag_mask.get(idx).copied().unwrap_or(false) {
            if !class.is_empty() {
                class.push(' ');
            }
            class.push_str("diag-range");
        }

        if class != current_class {
            if !current_text.is_empty() {
                spans.push((current_text.clone(), current_class.clone()));
                current_text.clear();
            }
            current_class = class;
        }
        current_text.push(*ch);
    }

    if !current_text.is_empty() || spans.is_empty() {
        spans.push((current_text, current_class));
    }

    spans
}

fn diag_messages_for_line(line_index: u32, diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut messages = Vec::new();
    let mut seen = HashSet::new();
    for diag in diagnostics {
        if diag.range.start.line == line_index {
            let message = diag.message.clone();
            if seen.insert(message.clone()) {
                messages.push(message);
            }
        }
    }
    messages
}

fn parse_px(value: &str) -> f64 {
    value.trim().trim_end_matches("px").parse::<f64>().unwrap_or(0.0)
}

/// Code text prefilled with code, the user can modify it.
///
/// On change, it reruns the language serve, updates the hovers
/// and the diagnostics.
#[component]
fn CodeInput<'a>(
    initial_code: &'a str,
    #[prop(optional, into)] wrapper_class: Option<String>,
) -> impl IntoView {
    let code = RwSignal::new(initial_code.to_string());
    let analysis = RwSignal::new(Arc::new(analyze_source(initial_code)));

    let hover_text = RwSignal::new(None::<String>);
    let hover_pos = RwSignal::new((0.0_f64, 0.0_f64));
    let char_width = RwSignal::new(0.0_f64);
    let line_height = RwSignal::new(0.0_f64);
    let padding_left = RwSignal::new(0.0_f64);
    let padding_top = RwSignal::new(0.0_f64);
    let padding_bottom = RwSignal::new(0.0_f64);
    let did_measure = RwSignal::new(false);

    let input_ref = NodeRef::<leptos::html::Textarea>::new();
    let overlay_ref = NodeRef::<leptos::html::Div>::new();
    let wrapper_ref = NodeRef::<leptos::html::Div>::new();
    let measure_ref = NodeRef::<leptos::html::Span>::new();

    Effect::new(move |_| {
        let src = code.get();
        analysis.set(Arc::new(analyze_source(&src)));
    });

    Effect::new(move |_| {
        if did_measure.get() {
            return;
        }

        let Some(measure) = measure_ref.get() else {
            return;
        };
        if let Some(input) = input_ref.get() {
            if let Ok(Some(style)) = window().get_computed_style(&input) {
                padding_left
                    .set(parse_px(&style.get_property_value("padding-left").unwrap_or_default()));
                padding_top
                    .set(parse_px(&style.get_property_value("padding-top").unwrap_or_default()));
                padding_bottom
                    .set(parse_px(&style.get_property_value("padding-bottom").unwrap_or_default()));
            }
        }

        let rect = measure
            .unchecked_ref::<Element>()
            .get_bounding_client_rect();
        char_width.set(rect.width());
        let computed_height = rect.height();
        if computed_height > 0.0 {
            line_height.set(computed_height);
        }
        did_measure.set(true);
    });

    let sync_scroll = move || {
        if let (Some(input), Some(overlay)) = (input_ref.get(), overlay_ref.get()) {
            overlay.set_scroll_top(input.scroll_top());
            overlay.set_scroll_left(input.scroll_left());
        }
    };

    let on_input = move |ev| {
        let value = event_target_value(&ev);
        let line_count = value.split('\n').count().max(1) as f64;
        code.set(value);
        if let Some(input) = input_ref.get() {
            let scroll_height = input.scroll_height() as f64;
            let adjusted = scroll_height - padding_top.get_untracked() - padding_bottom.get_untracked();
            if adjusted > 0.0 {
                line_height.set(adjusted / line_count);
            }
        }
        sync_scroll();
    };

    let on_mouse_move = move |ev: MouseEvent| {
        let Some(input) = input_ref.get() else {
            return;
        };
        let Some(wrapper) = wrapper_ref.get() else {
            return;
        };

        let rect = input
            .unchecked_ref::<Element>()
            .get_bounding_client_rect();
        let mut x = ev.client_x() as f64 - rect.left() - padding_left.get_untracked();
        let mut y = ev.client_y() as f64 - rect.top() - padding_top.get_untracked();

        x += input.scroll_left() as f64;
        y += input.scroll_top() as f64;

        if x < 0.0 || y < 0.0 {
            hover_text.set(None);
            return;
        }

        let line_h = line_height.get_untracked();
        let char_w = char_width.get_untracked();
        if line_h <= 0.0 || char_w <= 0.0 {
            return;
        }

        let line = (y / line_h).floor() as u32;
        let mut character = (x / char_w).floor() as u32;

        let src = code.get_untracked();
        let lines: Vec<&str> = src.split('\n').collect();
        if line as usize >= lines.len() {
            hover_text.set(None);
            return;
        }
        let max_char = lines[line as usize].chars().count() as u32;
        if character > max_char {
            character = max_char;
        }

        let analysis = analysis.get_untracked();
        if let Some(info) = analysis.hover(Position { line, character }) {
            hover_text.set(Some(render_hover_text(info)));
            let wrapper_rect = wrapper
                .unchecked_ref::<Element>()
                .get_bounding_client_rect();
            let popup_x = ev.client_x() as f64 - wrapper_rect.left();
            let popup_y = ev.client_y() as f64 - wrapper_rect.top();
            hover_pos.set((popup_x, popup_y));
        } else {
            hover_text.set(None);
        }
    };

    let on_mouse_leave = move |_| {
        hover_text.set(None);
    };

    let overlay_view = move || {
        let analysis = analysis.get();
        let diagnostics = &analysis.diagnostics;
        let src = code.get();
        let lines: Vec<&str> = src.split('\n').collect();
        lines
            .into_iter()
            .enumerate()
            .map(|(line_idx, line)| {
                let line_index = line_idx as u32;
                let spans = build_line_spans(line, line_index, diagnostics);
                let messages = diag_messages_for_line(line_index, diagnostics);
                view! {
                    <div class="code-line">
                        <span class="code-line-text">
                            {spans
                                .into_iter()
                                .map(|(text, class_name)| view! { <span class=class_name>{text}</span> }.into_view())
                                .collect_view()}
                        </span>
                        {(!messages.is_empty()).then(|| {
                            view! {
                                <span class="diag-line-messages">
                                    {messages
                                        .into_iter()
                                        .map(|message| view! { <span class="diag-virtual">{message}</span> })
                                        .collect_view()}
                                </span>
                            }
                        })}
                    </div>
                }
            })
            .collect_view()
    };

    let wrapper_class = wrapper_class.unwrap_or_default();

    view! {
        <div class=format!("code-wrapper {}", wrapper_class) node_ref=wrapper_ref>
            <div class="code-overlay" node_ref=overlay_ref>
                {overlay_view}
            </div>
            <textarea
                class="code-input"
                node_ref=input_ref
                on:input=on_input
                on:scroll=move |_| sync_scroll()
                on:mousemove=on_mouse_move
                on:mouseleave=on_mouse_leave
                prop:value=move || code.get()
            />
            <span class="measure-char" node_ref=measure_ref>"M"</span>
            {move || {
                hover_text.get().map(|text| {
                    let (x, y) = hover_pos.get();
                    view! {
                        <div
                            class="hover-popup"
                            style=format!("left: {:.2}px; top: {:.2}px;", x + 12.0, y + 12.0)
                        >
                            {text}
                        </div>
                    }
                })
            }}
        </div>
    }
}

/// Default Home Page
#[component]
pub fn Home() -> impl IntoView {
    let snippet_1 = r#"
import torch
    
def matmul(x, y):
    B, X, Y, Z = 32, 12, 8, 2
    x = torch.Tensor(B, X, Y)
    y = torch.Tensor(Y, Z)
    z = x @ y.T
    return z 
"#;
    let snippet_2 = r#"
from jaxtyping import Float
import torch
    
def matmul_permute(x: Float[torch.Tensor, "B X Y"], y):
    B, X, Y, Z = 32, 12, 8, 2
    Y, Z = y.shape
    z = x @ y
    w = z.permute(1, 2, 0) @ torch.zeros([B, X])
    return w
"#;

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

                <h1>"shapels: a primer"</h1>

                <p>"shapels provides static analysis for torch operations."</p>
                <p>"Check the following snippet:"</p>

                <CodeInput attr:spellcheck="false" initial_code=snippet_1/>

                <p>"As you can see, shapels emits a diagnostic because it can assert that the two tensors cannot be multiplied."</p>
                <p><em>"The code above is interactive"</em>": remove the "<inline-code>".T"</inline-code>" and see what happens"</p>
                <p>"As you may have noticed, you can hover over the variables to display their inferred shape."</p>
                <p><em>"Sounds good, how do I install this in my favourite editor of choice?"</em></p>
                <p>
                    "Head over to the "
                    <a href="https://github.com/carrascomj/shapels?tab=readme-ov-file#editor-support">"Editor support section"</a>
                    " to find setup instructions."
                </p>
                <p>"But how does it work?"</p>
                <p>
                    "Shapels first requires an initial tensor shape calls. Then, by looking at the tensor operations, shapels computes the shape of each tensor statically. In the case above, it was able to get the initial shape from the"
                    <inline-code>"torch.Tensor"</inline-code>
                    "call. But that's not the only way!"
                </p>
                <CodeInput attr:spellcheck="false" initial_code=snippet_2/>
                <p>
                    "As you can see, one can use "
                    <a href="https://docs.kidger.site/jaxtyping/">"jaxtyping"</a>
                    " to annotate the shapes of the tensors. Other options are unrolling the shape, like the case of "
                    <inline-code>"y"</inline-code>
                    " above or common creation ops like"
                    <inline-code>"torch.zeros"</inline-code>
                    ", "
                    <inline-code>"torch.ones"</inline-code>
                    ", etc."
                </p>
                <p>
                    "With that said, not all torch operations are understood by shapels yet. If you find one that you would like to have implemented, feel free"
                    <a href="https://github.com/carrascomj/shapels/issues">" open an issue"</a>
                    "!"
                </p>
            </div>
        </ErrorBoundary>
    }
}

/// A big empty playground for testing code in a single page.
#[component]
pub fn Playground() -> impl IntoView {
    let prefilled_code_snippet = r#"from jaxtyping import Float
import torch


class UserLinear(torch.nn.Module):
    def __init__(self):
        super().__init__()

    def forward(x, y):
        return x @ y.permute(1, 0)

    def annotated_method(x: Float[Tensor, "B X Y"], y: Float[Tensor, "Z Y"]) -> tuple[Float[Tensor, "B X Z"], Float[Tensor, "Z Y"]]:
        """Hints in the signature will shortcircuit inference at the caller site."""
        some_ones = torch.ones_like(y)
        return x @ y.permute(1, 0), some_ones


def some_function(x: Float[torch.Tensor, "B X Y"], y, linear: UserLinear):
    # at this point, the shape of x is known because of jaxtyping's annotation
    # y shape can be inferred because `x` (and thus x.size(2)) is known
    y = torch.zeros(32, x.size(2))
    # shapels here jumps to UserLinear.forward and runs inference given
    # the shapes of x and y (that are known at this point)
    z = linear(x, y)
    # fine: x has a shape identical to annotated_method's 1st arg hint and
    # y is alpha compatible (same dimensions, different name) with the 2nd arg hint
    fine, not_ones = linear.annotated_method(x, y)
    # this would not be fine since now the second argument is not compatible with the type
    # hint, so shapels reports a diagnostics
    not_fine = linear.annotated_method(x, x)
"#;

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

                <h1>"shapels playground"</h1>

                <p>
                    Edit the code below and hover with the mouse over the tensors, running <a href="https://github.com/carrascomj/shapels/issues">" shapels "</a>
                    in realtime!
                </p>
                <CodeInput
                    attr:spellcheck="false"
                    initial_code=prefilled_code_snippet
                    wrapper_class="code-wrapper--playground"
                />
            </div>
        </ErrorBoundary>
    }
}
