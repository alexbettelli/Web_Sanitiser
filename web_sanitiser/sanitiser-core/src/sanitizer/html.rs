//! Regole di sanitizzazione della struttura HTML.

use scraper::{ElementRef, Html, Node};

use crate::config::ScriptPolicy;
use crate::report::SanitiseAction;

/// Rimuove gli attributi il cui nome locale inizia con `on`, ignorando il caso.
///
/// Conserva gli elementi e il loro contenuto. Restituisce un'azione per ogni
/// attributo rimosso; il frammento riportato è ricostruito dai valori del DOM.
/// Una sostituzione vuota indica che l'attributo è stato eliminato.
pub fn remove_event_handlers(document: &mut Html) -> Vec<SanitiseAction> {
    // Prima leggiamo il DOM e raccogliamo gli identificativi e le posizioni.
    // In questo modo possiamo modificarlo nel ciclo successivo.
    let elements: Vec<_> = document
        .tree
        .root()
        .descendants()
        .filter_map(ElementRef::wrap)
        .filter(|element| {
            element
                .value()
                .attrs
                .iter()
                .any(|(name, _)| is_event_handler(name.local.as_ref()))
        })
        .map(|element| (element.id(), element_location(element)))
        .collect();

    let mut actions = Vec::new();

    for (node_id, location) in elements {
        let Some(mut node) = document.tree.get_mut(node_id) else {
            continue;
        };
        let Node::Element(element) = node.value() else {
            continue;
        };

        // retain conserva solo gli attributi per cui restituiamo true.
        element.attrs.retain(|(name, value)| {
            if !is_event_handler(name.local.as_ref()) {
                return true;
            }

            let attribute_name = match &name.prefix {
                Some(prefix) => format!("{prefix}:{}", name.local),
                None => name.local.to_string(),
            };
            let escaped_value = value.replace('&', "&amp;").replace('"', "&quot;");

            actions.push(SanitiseAction {
                rule: "remove-event-handler".to_string(),
                location: format!("{location}/@{attribute_name}"),
                original_fragment: format!("{attribute_name}=\"{escaped_value}\""),
                replacement: String::new(),
            });

            false
        });
    }

    actions
}

/// Rimuove gli elementi script non autorizzati, insieme al loro contenuto.
///
/// Gli script HTML esterni sono verificati tramite src, quelli inline tramite
/// il contenuto esatto. Gli script in altri namespace, come SVG, sono rimossi.
/// Questa regola non scarica né esegue JavaScript.
pub fn remove_disallowed_scripts(
    document: &mut Html,
    policy: &ScriptPolicy,
) -> Vec<SanitiseAction> {
    // Conserviamo frammenti e posizioni prima di rimuovere i nodi, così gli
    // indici dei fratelli non cambiano mentre costruiamo il report.
    let scripts: Vec<_> = document
        .tree
        .root()
        .descendants()
        .filter_map(ElementRef::wrap)
        .filter(|element| is_script(*element) && !is_allowed_script(*element, policy))
        .map(|element| {
            let action = SanitiseAction {
                rule: "remove-disallowed-script".to_string(),
                location: element_location(element),
                original_fragment: element.html(),
                replacement: String::new(),
            };

            (element.id(), action)
        })
        .collect();

    let mut actions = Vec::new();

    for (node_id, action) in scripts {
        if let Some(mut node) = document.tree.get_mut(node_id) {
            // Il sottoalbero viene scollegato: non comparirà nell'HTML prodotto.
            node.detach();
            actions.push(action);
        }
    }

    actions
}

fn is_script(element: ElementRef<'_>) -> bool {
    element.value().name().eq_ignore_ascii_case("script")
}

fn is_allowed_script(element: ElementRef<'_>, policy: &ScriptPolicy) -> bool {
    let namespace: &str = element.value().name.ns.as_ref();
    if namespace != "http://www.w3.org/1999/xhtml" {
        return false;
    }

    // Anche src="" identifica un riferimento esterno: non deve ripiegare
    // sull'allow-list inline quando il suo URL non è valido.
    match element.value().attr("src") {
        Some(source) => policy.allows_source(source),
        None => policy.allows_inline(&element.text().collect::<String>()),
    }
}

fn is_event_handler(name: &str) -> bool {
    name.get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
}

/// Costruisce un percorso nel DOM con indici tra fratelli dello stesso tipo.
fn element_location(element: ElementRef<'_>) -> String {
    let mut parts = Vec::new();

    // I template contengono un nodo Fragment: lo attraversiamo per risalire
    // comunque agli elementi antenati e ottenere il percorso completo.
    let ancestors = element.ancestors().filter_map(ElementRef::wrap);
    for element in std::iter::once(element).chain(ancestors) {
        let index = element
            .prev_siblings()
            .filter_map(ElementRef::wrap)
            .filter(|sibling| sibling.value().name == element.value().name)
            .count()
            + 1;

        parts.push(format!("{}[{index}]", element.value().name()));
    }

    parts.reverse();
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use scraper::Selector;

    use super::{remove_disallowed_scripts, remove_event_handlers};
    use crate::config::{Policy, ScriptPolicy};
    use crate::parser::html::{parse_html, serialize_html};

    #[test]
    fn removes_handlers_and_preserves_elements_text_and_other_attributes() {
        let input = concat!(
            "<main><button id=\"ok\" ONCLICK=\"evil()\" onfocus=\"other()\" ",
            "data-onclick=\"keep\">Premi <b>qui</b></button>",
            "<img src=\"x.png\" onerror=\"evil()\"></main>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_event_handlers(&mut document);

        assert_eq!(actions.len(), 3);
        assert!(actions.iter().all(|action| {
            action.rule == "remove-event-handler" && action.replacement.is_empty()
        }));

        let selector = Selector::parse("button").unwrap();
        let button = document.select(&selector).next().unwrap();
        assert_eq!(button.value().attr("onclick"), None);
        assert_eq!(button.value().attr("onfocus"), None);
        assert_eq!(button.value().attr("id"), Some("ok"));
        assert_eq!(button.value().attr("data-onclick"), Some("keep"));
        assert_eq!(button.inner_html(), "Premi <b>qui</b>");

        let selector = Selector::parse("img").unwrap();
        let image = document.select(&selector).next().unwrap();
        assert_eq!(image.value().attr("onerror"), None);
        assert_eq!(image.value().attr("src"), Some("x.png"));

        let output = serialize_html(&document);
        assert!(remove_event_handlers(&mut document).is_empty());
        assert_eq!(serialize_html(&document), output);
    }

    #[test]
    fn reports_attribute_and_distinguishes_repeated_elements() {
        let input = concat!(
            "<div><button>Uno</button>testo<span></span><!-- commento -->",
            "<button onclick='alert(\"x\") &amp;&amp; evil()'>Due</button></div>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_event_handlers(&mut document);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].location,
            "/html[1]/body[1]/div[1]/button[2]/@onclick"
        );
        assert_eq!(
            actions[0].original_fragment,
            "onclick=\"alert(&quot;x&quot;) &amp;&amp; evil()\""
        );
    }

    #[test]
    fn leaves_clean_document_unchanged() {
        let input = "<p class=\"text\" data-onclick=\"keep\">Città &amp; mare</p>";
        let mut document = parse_html(input.as_bytes()).unwrap();
        let before = serialize_html(&document);

        let actions = remove_event_handlers(&mut document);

        assert!(actions.is_empty());
        assert_eq!(serialize_html(&document), before);
    }

    #[test]
    fn removes_handlers_from_svg_elements() {
        let input = b"<svg onload=\"evil()\"><circle ONCLICK=\"evil()\"></circle></svg>";
        let mut document = parse_html(input).unwrap();

        let actions = remove_event_handlers(&mut document);

        assert_eq!(actions.len(), 2);
        let selector = Selector::parse("svg, circle").unwrap();
        for element in document.select(&selector) {
            assert_eq!(element.value().attr("onload"), None);
            assert_eq!(element.value().attr("onclick"), None);
        }
    }

    #[test]
    fn removes_all_scripts_by_default_and_reports_original_positions() {
        let input = concat!(
            "<main><SCRIPT>evil()</SCRIPT><p>Da conservare</p>",
            "<script src=\"https://cdn.example/app.js\"></script></main>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_disallowed_scripts(&mut document, &ScriptPolicy::default());

        assert_eq!(
            document.html(),
            "<html><head></head><body><main><p>Da conservare</p></main></body></html>"
        );
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].location, "/html[1]/body[1]/main[1]/script[1]");
        assert_eq!(actions[1].location, "/html[1]/body[1]/main[1]/script[2]");
        assert_eq!(actions[0].original_fragment, "<script>evil()</script>");
        assert_eq!(
            actions[1].original_fragment,
            "<script src=\"https://cdn.example/app.js\"></script>"
        );
        assert!(actions.iter().all(|action| {
            action.rule == "remove-disallowed-script" && action.replacement.is_empty()
        }));
        assert!(remove_disallowed_scripts(&mut document, &ScriptPolicy::default()).is_empty());
    }

    #[test]
    fn keeps_external_script_allowed_by_toml_policy() {
        let policy = Policy::from_toml(include_str!("../../../policy.example.toml")).unwrap();
        let input = concat!(
            "<script src=\"HTTPS://CDN.EXAMPLE:443/assets/app.js?v=1\" defer>",
            "</script><p>Pagina</p>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();
        let before = serialize_html(&document);

        let actions = remove_disallowed_scripts(&mut document, &policy.scripts);

        assert!(actions.is_empty());
        assert_eq!(serialize_html(&document), before);
    }

    #[test]
    fn rejects_external_urls_that_only_resemble_an_allowed_url() {
        let policy = ScriptPolicy {
            allowed_sources: vec!["https://cdn.example/assets/app.js?v=1".to_string()],
            ..Default::default()
        };

        for source in [
            "https://cdn.example.evil.test/assets/app.js?v=1",
            "https://cdn.example@evil.test/assets/app.js?v=1",
            "https://cdn.example/assets/app.js.evil?v=1",
            "https://cdn.example/other.js?v=1",
            "https://cdn.example/assets/app.js?v=2",
            "https://cdn.example/assets/app.js?v=1&evil=true",
            "http://cdn.example/assets/app.js?v=1",
            "https://cdn.example:444/assets/app.js?v=1",
        ] {
            let input = format!("<script src=\"{source}\"></script>");
            let mut document = parse_html(input.as_bytes()).unwrap();

            let actions = remove_disallowed_scripts(&mut document, &policy);

            assert_eq!(actions.len(), 1, "URL: {source}");
            assert!(!document.html().contains("<script"), "URL: {source}");
        }
    }

    #[test]
    fn rejects_unsupported_sources_even_when_explicitly_listed() {
        for source in [
            "",
            "/assets/app.js",
            "assets/app.js",
            "//cdn.example/app.js",
            "https:cdn.example/app.js",
            "https:/cdn.example/app.js",
            "javascript:evil()",
            "data:text/javascript,evil()",
            "file:///app.js",
            "ftp://cdn.example/app.js",
            "https://user:password@cdn.example/app.js",
            "https://[invalid/app.js",
        ] {
            let policy = ScriptPolicy {
                allowed_sources: vec![source.to_string()],
                allowed_inline: vec!["trusted()".to_string()],
            };
            let input = format!("<script src=\"{source}\">trusted()</script>");
            let mut document = parse_html(input.as_bytes()).unwrap();

            let actions = remove_disallowed_scripts(&mut document, &policy);

            assert_eq!(actions.len(), 1, "URL: {source}");
            assert!(!document.html().contains("<script"), "URL: {source}");
        }
    }

    #[test]
    fn authorizes_only_exact_inline_content() {
        let policy = ScriptPolicy {
            allowed_inline: vec!["trusted()".to_string()],
            ..Default::default()
        };
        let input = concat!(
            "<main><script>trusted()</script>",
            "<script>trusted();evil()</script>",
            "<script> trusted()</script>",
            "<script src=\"\">trusted()</script></main>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_disallowed_scripts(&mut document, &policy);

        assert_eq!(actions.len(), 3);
        assert_eq!(
            document.html(),
            "<html><head></head><body><main><script>trusted()</script></main></body></html>"
        );
    }

    #[test]
    fn removes_svg_and_template_scripts_with_complete_locations() {
        let policy = ScriptPolicy {
            allowed_sources: vec!["https://cdn.example/app.js".to_string()],
            allowed_inline: vec!["trusted()".to_string()],
        };
        let input = concat!(
            "<main><svg><script href=\"https://cdn.example/app.js\" ",
            "src=\"https://cdn.example/app.js\">trusted()</script></svg>",
            "<template><script>evil()</script></template></main>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_disallowed_scripts(&mut document, &policy);

        assert_eq!(actions.len(), 2);
        assert!(!document.html().contains("<script"));
        assert_eq!(
            actions[0].location,
            "/html[1]/body[1]/main[1]/svg[1]/script[1]"
        );
        assert_eq!(
            actions[1].location,
            "/html[1]/body[1]/main[1]/template[1]/script[1]"
        );
    }

    #[test]
    fn removes_event_handlers_even_from_an_authorized_script() {
        let policy = ScriptPolicy {
            allowed_sources: vec!["https://cdn.example/app.js".to_string()],
            ..Default::default()
        };
        let input = b"<script src=\"https://cdn.example/app.js\" onload=\"evil()\"></script>";
        let mut document = parse_html(input).unwrap();

        let handler_actions = remove_event_handlers(&mut document);
        let script_actions = remove_disallowed_scripts(&mut document, &policy);

        assert_eq!(handler_actions.len(), 1);
        assert!(script_actions.is_empty());
        assert!(document.html().contains("<script src="));
        assert!(!document.html().contains("onload"));
    }
}
