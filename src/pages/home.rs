use leptos::prelude::window;
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys::{HtmlElement, HtmlSpanElement, HtmlTextAreaElement};
use lsp_types::{Diagnostic, Position, Range};
use shapels::analyze_source;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Debug, Clone)]
struct LineRender {
    segments: Vec<RenderSegment>,
    virtual_texts: Vec<String>,
}

#[derive(Debug, Clone)]
struct RenderSegment {
    text: String,
    has_diag: bool,
}

fn highlight_tokens(text: &str) -> Vec<(String, Option<&'static str>)> {
    let keywords = ["import", "from", "def", "return"];
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut string_delim = '\0';
    while let Some(ch) = chars.next() {
        if in_string {
            buf.push(ch);
            if ch == string_delim {
                out.push((buf.clone(), Some("hl-string")));
                buf.clear();
                in_string = false;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            if !buf.is_empty() {
                out.push((buf.clone(), None));
                buf.clear();
            }
            in_string = true;
            string_delim = ch;
            buf.push(ch);
            continue;
        }

        if ch.is_alphanumeric() || ch == '_' {
            buf.push(ch);
            // continue to build word
            continue;
        } else {
            if !buf.is_empty() {
                let cls = if keywords.contains(&buf.as_str()) {
                    Some("hl-keyword")
                } else {
                    None
                };
                out.push((buf.clone(), cls));
                buf.clear();
            }
            out.push((ch.to_string(), None));
        }
    }
    if !buf.is_empty() {
        let cls = if keywords.contains(&buf.as_str()) {
            Some("hl-keyword")
        } else {
            None
        };
        out.push((buf, cls));
    }
    out
}

fn position_to_offset(src: &str, pos: &Position) -> Option<usize> {
    let mut offset = 0usize;
    for (line_idx, line) in src.split_inclusive('\n').enumerate() {
        if line_idx as u32 == pos.line {
            let mut char_count = 0usize;
            for (byte_idx, _) in line.char_indices() {
                if char_count == pos.character as usize {
                    return Some(offset + byte_idx);
                }
                char_count += 1;
            }
            if char_count == pos.character as usize {
                return Some(offset + line.len());
            }
            return None;
        }
        offset += line.len();
    }

    // Allow positions that point to the end of the file
    if pos.line as usize == src.lines().count() && pos.character == 0 {
        return Some(src.len());
    }

    None
}

fn range_to_offsets(src: &str, range: &Range) -> Option<(usize, usize)> {
    let start = position_to_offset(src, &range.start)?;
    let end = position_to_offset(src, &range.end)?;
    Some((start.min(end), end.max(start)))
}

fn render_hover_text(info: &shapels::HoverInfo) -> String {
    if let Some(shape) = &info.shape {
        format!(
            "`{}`: {}",
            shape.render(),
            shape.dtype.as_deref().unwrap_or("")
        )
    } else {
        String::from("hover unavailable")
    }
}

fn split_lines_with_metadata(
    code: &str,
    diagnostics: &[Diagnostic],
    hover_entries: &[(Range, shapels::HoverInfo)],
) -> Vec<LineRender> {
    let mut diag_ranges: Vec<(usize, usize, String)> = diagnostics
        .iter()
        .filter_map(|d| range_to_offsets(code, &d.range).map(|(s, e)| (s, e, d.message.clone())))
        .collect();
    diag_ranges.sort_by_key(|(s, _, _)| *s);

    let mut hover_ranges: Vec<(usize, usize, String)> = hover_entries
        .iter()
        .filter_map(|(range, info)| {
            range_to_offsets(code, range).map(|(s, e)| (s, e, render_hover_text(info)))
        })
        .collect();
    hover_ranges.sort_by_key(|(s, _, _)| *s);

    let mut lines = Vec::new();
    let mut line_start = 0usize;

    for line in code.split('\n') {
        let line_len = line.len();
        let line_end = line_start + line_len;

        let mut boundaries: Vec<usize> = vec![line_start, line_end];
        for (s, e, _) in diag_ranges.iter().chain(hover_ranges.iter()) {
            let start = (*s).max(line_start).min(line_end);
            let end = (*e).max(line_start).min(line_end);
            if start < end {
                boundaries.push(start);
                boundaries.push(end);
            }
        }

        boundaries.sort_unstable();
        boundaries.dedup();

        let mut segments = Vec::new();
        for window in boundaries.windows(2) {
            let seg_start = window[0];
            let seg_end = window[1];
            if seg_start >= seg_end {
                continue;
            }
            let text = line[(seg_start - line_start)..(seg_end - line_start)].to_string();

            let has_diag = diag_ranges
                .iter()
                .any(|(s, e, _)| seg_start < *e && seg_end > *s);

            segments.push(RenderSegment { text, has_diag });
        }

        if segments.is_empty() {
            segments.push(RenderSegment {
                text: line.to_string(),
                has_diag: false,
            });
        }

        let mut virtual_texts = HashSet::new();
        for (s, e, msg) in diag_ranges.iter() {
            if *s < line_end && *e > line_start {
                virtual_texts.insert(msg.clone());
            }
        }

        lines.push(LineRender {
            segments,
            virtual_texts: virtual_texts.into_iter().collect(),
        });

        // account for the stripped '\n'
        line_start = line_end + 1;
    }

    // preserve trailing empty line if code ends with newline
    if code.ends_with('\n') {
        lines.push(LineRender {
            segments: vec![RenderSegment {
                text: String::new(),
                has_diag: false,
            }],
            virtual_texts: Vec::new(),
        });
    }

    lines
}

/// Code text prefilled with code, the user can modify it.
///
/// On change, it reruns the language serve, updates the hovers
/// and the diagnostics.
#[component]
fn CodeInput<'a>(initial_code: &'a str) -> impl IntoView {
    let (code, set_code) = signal(initial_code.to_string());
    let text_ref = NodeRef::<leptos::html::Textarea>::new();
    let overlay_ref = NodeRef::<leptos::html::Pre>::new();
    let measure_ref = NodeRef::<leptos::html::Span>::new();
    let (hover_popup, set_hover_popup) = signal(None::<(usize, f64, String)>);
    let analysis_store = StoredValue::new_local(Rc::new(analyze_source(initial_code)));

    view! {
        <div class="code-wrapper">
            <pre class="code-overlay" aria-hidden="true" node_ref=overlay_ref>
                {move || {
                    // refresh analysis once per render
                    analysis_store.set_value(Rc::new(analyze_source(&code.get())));
                    let current = analysis_store.get_value();
                    split_lines_with_metadata(
                        &code.get(),
                        current.diagnostics.as_slice(),
                        current.hover_entries.as_slice(),
                    )
                    .into_iter()
                    .enumerate()
                    .map(|(_line_idx, line)| {
                            let segments = line.segments.into_iter().map(|segment| {
                                let range_class = if segment.has_diag {
                                    "diag-range"
                                } else {
                                    "diag-range diag-none"
                                };
                                let tokens = highlight_tokens(&segment.text);
                                view! {
                                    <span class="code-span">
                                        {tokens.into_iter().map(move |(txt, hl)| {
                                            let cls = hl
                                                .map(|c| format!("{range_class} {c}"))
                                                .unwrap_or_else(|| range_class.to_string());
                                            view! { <span class=cls>{txt}</span> }
                                        }).collect_view()}
                                    </span>
                                }
                                .into_view()
                            });

                            let virtuals: Vec<_> = line
                                .virtual_texts
                                .into_iter()
                                .map(|msg| {
                                    view! { <span class="diag-virtual">{" ⟫ "}{msg}</span> }
                                        .into_view()
                                })
                                .collect();

                        view! {
                            <div class="code-line">
                                <span class="code-line-text">{segments.collect_view()}</span>
                                <span class="diag-line-messages">{virtuals.into_iter().collect_view()}</span>
                            </div>
                        }
                        .into_view()
                    })
                    .collect_view()
                }}
            </pre>
            <textarea
                class="code-input"
                // update the signal on each keystroke
                bind:value=(code, set_code)
                spellcheck=false
                wrap="off"
                node_ref=text_ref
                on:scroll=move |_| {
                    if let (Some(textarea), Some(overlay)) = (text_ref.get(), overlay_ref.get()) {
                        overlay.set_scroll_left(textarea.scroll_left());
                        overlay.set_scroll_top(textarea.scroll_top());
                    }
                }
                on:mousemove=move |ev| {
                    if let (Some(textarea), Some(measure)) = (text_ref.get(), measure_ref.get()) {
                        if let Some((char_w, line_h, _pad_left, _pad_top)) =
                            measure_metrics(&textarea, &measure)
                        {
                            let x = ev.offset_x() as f64 + textarea.scroll_left() as f64;
                            let y = ev.offset_y() as f64 + textarea.scroll_top() as f64;
                            if char_w > 0.0 && line_h > 0.0 && x >= 0.0 && y >= 0.0 {
                                let line = (y / line_h).floor() as u32;
                                let character = (x / char_w).floor() as u32;
                                let pos = Position { line, character };
                                analysis_store.with_value(|analysis| {
                                    if let Some(info) = analysis.hover(pos) {
                                        set_hover_popup.set(Some((
                                            line as usize,
                                            x,
                                            render_hover_text(info),
                                        )));
                                    } else {
                                        set_hover_popup.set(None);
                                    }
                                });
                            }
                        }
                    }
                }
                on:mouseleave=move |_| set_hover_popup.set(None)
            ></textarea>
            <span class="measure-char" aria-hidden="true" node_ref=measure_ref>"M"</span>
            {move || {
                hover_popup
                    .get()
                    .map(|(line, x, text)| {
                        let (top, left) = if let (Some(textarea), Some(measure)) =
                            (text_ref.get(), measure_ref.get())
                        {
                        if let Some((char_w, line_h, pad_left, _pad_top)) =
                            measure_metrics(&textarea, &measure)
                            {
                                let scroll_top = textarea.scroll_top() as f64;
                                let scroll_left = textarea.scroll_left() as f64;
                                let top = (line as f64) * line_h - scroll_top;
                                let left = x - scroll_left + char_w * 0.5 + 8.0 + pad_left;
                                (top, left)
                            } else {
                                (0.0, 0.0)
                            }
                        } else {
                            (0.0, 0.0)
                        };
                        view! {
                            <div class="hover-popup" style=format!("top: {}px; left: {}px;", top, left)>
                                {text}
                            </div>
                        }
                    })
            }}
        </div>
    }
}

fn measure_metrics(
    textarea: &HtmlTextAreaElement,
    measure: &HtmlSpanElement,
) -> Option<(f64, f64, f64, f64)> {
    let m_elem: &HtmlElement = measure.unchecked_ref();
    let char_w = m_elem.offset_width() as f64;
    let line_h = m_elem.offset_height() as f64;

    let element: &HtmlElement = textarea.unchecked_ref();
    let window = window();
    let style = window.get_computed_style(element).ok()??;
    let pad_left = parse_px(style.get_property_value("padding-left").ok());
    let pad_top = parse_px(style.get_property_value("padding-top").ok());

    Some((char_w, line_h, pad_left, pad_top))
}

fn parse_px(value: Option<String>) -> f64 {
    value
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(0.0)
}
/// Default Home Page
#[component]
pub fn Home() -> impl IntoView {
    let snippet_1= r#"
import torch
    
def matmul(x, y):
    B, X, Y, Z = 32, 12, 8, 2
    x = torch.Tensor(B, X, Y)
    y = torch.Tensor(Y, Z)
    z = x @ y.T
    return z 
"#;
    let snippet_2= r#"
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
