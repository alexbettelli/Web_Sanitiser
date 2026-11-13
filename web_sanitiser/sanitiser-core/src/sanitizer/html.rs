//! Regole di sanitizzazione della struttura HTML.
//!
//! Il chiamante passa il DOM costruito dal parser. Le funzioni pubbliche
//! lo modificano direttamente e restituiscono le azioni da aggiungere al report.
//! Le funzioni private riconoscono gli elementi, verificano le autorizzazioni
//! e costruiscono le posizioni delle modifiche nel documento.

use scraper::{ElementRef, Html, Node};

use crate::config::ScriptPolicy;
use crate::report::SanitiseAction;

/// Rimuove gli attributi il cui nome locale inizia con `on`, ignorando il caso.
///
/// Riceve il DOM tramite `&mut Html`: può modificarlo, mentre il chiamante
/// continua a possederlo. Conserva gli elementi, il testo e gli altri attributi.
/// Per esempio, da `<button onclick="evil()">Premi</button>` elimina solo onclick.
///
/// Prima raccoglie gli elementi interessati, poi ne modifica gli attributi.
/// Restituisce un'azione per ogni attributo rimosso: il chiamante potrà unire
/// queste azioni al report e passare il DOM alla regola successiva.
/// Il frammento è ricostruito dal DOM; la sostituzione vuota indica la rimozione.
pub fn remove_event_handlers(document: &mut Html) -> Vec<SanitiseAction> {
    // Fase 1: leggiamo il DOM senza modificarlo. Separare lettura e modifica
    // evita di mantenere prestiti immutabili dell'albero mentre lo mutiamo.
    let elements: Vec<_> = document
        .tree
        .root()
        // Visita la radice e tutti i suoi discendenti, compresi testo e commenti.
        .descendants()
        // wrap restituisce Some solo per i nodi elemento: gli altri sono scartati.
        .filter_map(ElementRef::wrap)
        .filter(|element| {
            // value dà accesso ai dati dell'elemento, tra cui la lista attrs.
            // any risponde: "Esiste almeno un attributo evento su questo tag?"
            // Qui usiamo solo il nome; il valore dell'attributo è ignorato con _.
            element
                .value()
                .attrs
                .iter()
                .any(|(name, _)| is_event_handler(name.local.as_ref()))
        })
        // Per ogni elemento selezionato conserviamo ID e posizione nel DOM.
        // Il vettore contiene queste coppie, non copie degli elementi HTML.
        .map(|element| (element.id(), element_location(element)))
        .collect();

    // Le azioni descrivono le modifiche effettuate; non sono ancora testo JSON.
    let mut actions = Vec::new();

    // Fase 2: la lettura iniziale è terminata e possiamo modificare il DOM.
    for (node_id, location) in elements {
        // Recupera il nodo modificabile tramite il suo ID. Se manca, passa oltre.
        let Some(mut node) = document.tree.get_mut(node_id) else {
            continue;
        };
        // Node è un enum: prendiamo i dati del caso Element, saltando altri tipi.
        let Node::Element(element) = node.value() else {
            continue;
        };

        // retain chiama questa closure per ogni attributo:
        // true = conserva l'attributo; false = rimuovi l'attributo.
        element.attrs.retain(|(name, value)| {
            // name.local è il nome senza prefisso, per esempio "onclick".
            if !is_event_handler(name.local.as_ref()) {
                // Questo return termina solo la closure per l'attributo corrente,
                // non l'intera remove_event_handlers. Gli attributi normali restano.
                return true;
            }

            // Siamo nel caso da rimuovere. Ricostruiamo il nome completo per il
            // report, includendo un eventuale prefisso come "xlink:".
            let attribute_name = match &name.prefix {
                Some(prefix) => format!("{prefix}:{}", name.local),
                None => name.local.to_string(),
            };
            // Il parser ha già interpretato le entità HTML. Per rappresentare
            // il valore tra virgolette nel report, riapplichiamo le escape.
            let escaped_value = value.replace('&', "&amp;").replace('"', "&quot;");

            // Registriamo cosa eliminiamo e dove, prima che retain lo rimuova.
            // @ distingue la posizione di un attributo da quella di un elemento.
            actions.push(SanitiseAction {
                rule: "remove-event-handler".to_string(),
                location: format!("{location}/@{attribute_name}"),
                original_fragment: format!("{attribute_name}=\"{escaped_value}\""),
                replacement: String::new(),
            });

            // Ultima espressione della closure: false ordina la rimozione.
            false
        });
    }

    // Ultima espressione della funzione: restituisce il vettore al chiamante.
    actions
}

/// Rimuove gli elementi script non autorizzati, insieme al loro contenuto.
///
/// Riceve il DOM modificabile e la policy in sola lettura. A differenza della
/// regola sugli eventi, può scollegare un intero elemento e i suoi discendenti.
///
/// is_script riconosce il tag; is_allowed_script decide se può rimanere.
/// Gli script HTML esterni sono verificati tramite src, quelli inline tramite
/// il contenuto esatto. Gli script in altri namespace, come SVG, sono rimossi.
/// Liste di autorizzazione vuote comportano la rimozione di tutti gli script.
///
/// Restituisce le azioni da aggiungere al report. Il DOM modificato potrà poi
/// essere serializzato dal chiamante. Non scarica né esegue JavaScript.
pub fn remove_disallowed_scripts(
    document: &mut Html,
    policy: &ScriptPolicy,
) -> Vec<SanitiseAction> {
    // Fase 1: individuiamo gli script da rimuovere e prepariamo i dati del report.
    // Acquisire le posizioni prima delle rimozioni mantiene gli indici originali:
    // il secondo script rimane script[2] anche se elimineremo il primo.
    let scripts: Vec<_> = document
        .tree
        .root()
        .descendants()
        // Considera solo i nodi elemento, come nella regola sugli eventi.
        .filter_map(ElementRef::wrap)
        // && richiede entrambe le condizioni: è uno script e NON è autorizzato.
        // filter seleziona i candidati, ma non li cancella dal documento.
        // *element passa la vista dell'elemento alle funzioni, senza copiare il DOM.
        .filter(|element| is_script(*element) && !is_allowed_script(*element, policy))
        .map(|element| {
            // element.html() conserva la rappresentazione HTML dello script
            // prima della rimozione. Non è necessariamente la grafia del file.
            let action = SanitiseAction {
                rule: "remove-disallowed-script".to_string(),
                location: element_location(element),
                original_fragment: element.html(),
                replacement: String::new(),
            };

            // Salviamo l'ID per ritrovare il nodo nel successivo ciclo modificabile.
            (element.id(), action)
        })
        .collect();

    let mut actions = Vec::new();

    // Fase 2: recuperiamo ogni candidato e lo scollegiamo dal documento.
    for (node_id, action) in scripts {
        if let Some(mut node) = document.tree.get_mut(node_id) {
            // detach scollega tutto il sottoalbero: non toglie solo gli attributi.
            // L'elemento e il suo contenuto non compariranno nell'HTML prodotto.
            node.detach();
            // Aggiungiamo l'azione dopo aver effettuato la rimozione.
            actions.push(action);
        }
    }

    // Il DOM è stato modificato direttamente; restituiamo solo le azioni.
    actions
}

/// Rimuove i meta refresh HTML, sia di ricaricamento sia di reindirizzamento.
///
/// Riceve il DOM modificabile e restituisce un'azione per ogni elemento rimosso.
/// La regola è restrittiva: elimina tutti i meta con http-equiv uguale a refresh,
/// senza interpretare il ritardo o l'eventuale URL contenuti in content.
/// Conserva gli altri meta, come charset e description, e il resto della pagina.
/// Il chiamante decide se applicarla; la funzione non effettua richieste di rete.
pub fn remove_meta_refresh(document: &mut Html) -> Vec<SanitiseAction> {
    // Fase 1: leggiamo il DOM e selezioniamo solo gli elementi meta refresh.
    // La posizione e il frammento vengono acquisiti prima delle rimozioni:
    // gli indici nel report si riferiscono quindi al documento ricevuto.
    let refreshes: Vec<_> = document
        .tree
        .root()
        .descendants()
        // Scartiamo i nodi di testo e i commenti: non cerchiamo stringhe HTML.
        .filter_map(ElementRef::wrap)
        .filter(|element| is_meta_refresh(*element))
        .map(|element| {
            let action = SanitiseAction {
                rule: "remove-meta-refresh".to_string(),
                location: element_location(element),
                original_fragment: element.html(),
                replacement: String::new(),
            };

            // L'ID serve per recuperare il nodo modificabile nel secondo ciclo.
            (element.id(), action)
        })
        .collect();

    let mut actions = Vec::new();

    // Fase 2: scollega ciascun meta selezionato e registra la rimozione.
    for (node_id, action) in refreshes {
        if let Some(mut node) = document.tree.get_mut(node_id) {
            // Eliminiamo l'intero elemento, non soltanto l'attributo http-equiv.
            // Il meta non comparirà nell'HTML prodotto da serialize_html.
            node.detach();
            actions.push(action);
        }
    }

    actions
}

/// Riconosce un meta refresh senza modificare il DOM.
/// È chiamata da remove_meta_refresh per decidere quali elementi scollegare.
/// Verifica il tag e il namespace HTML prima di leggere http-equiv, così un
/// attributo omonimo su un altro tipo di elemento non provoca la sua rimozione.
fn is_meta_refresh(element: ElementRef<'_>) -> bool {
    let tag = element.value();
    let namespace: &str = tag.name.ns.as_ref();

    if namespace != "http://www.w3.org/1999/xhtml" || !tag.name().eq_ignore_ascii_case("meta") {
        return false;
    }

    // Se manca http-equiv, is_some_and restituisce false e il meta rimane.
    // Ignoriamo il caso e, come scelta restrittiva, anche gli spazi ASCII esterni.
    // Il parser ha già decodificato eventuali entità HTML nel valore.
    tag.attr("http-equiv")
        .is_some_and(|value| value.trim_ascii().eq_ignore_ascii_case("refresh"))
}

/// Riconosce un elemento con nome locale script, senza modificarlo.
/// È chiamata da remove_disallowed_scripts prima del controllo sulla policy.
/// Il namespace non viene verificato qui: se ne occupa is_allowed_script.
fn is_script(element: ElementRef<'_>) -> bool {
    // value accede ai dati del tag; name() ne legge il nome locale.
    // Il confronto ignora le differenze ASCII tra maiuscole e minuscole.
    element.value().name().eq_ignore_ascii_case("script")
}

/// Decide se un elemento già riconosciuto come script è autorizzato.
///
/// Riceve una vista dell'elemento e la policy; restituisce soltanto true o false.
/// Non rimuove nodi: remove_disallowed_scripts userà questa risposta per farlo.
/// Controlla prima la famiglia dell'elemento, poi src oppure il contenuto inline.
fn is_allowed_script(element: ElementRef<'_>, policy: &ScriptPolicy) -> bool {
    // name.ns identifica il namespace, cioè la famiglia del tag (HTML, SVG, ...).
    // as_ref permette di leggerne il nome come &str.
    let namespace: &str = element.value().name.ns.as_ref();
    // Questa stringa è l'identificatore convenzionale del namespace HTML:
    // non è l'URL dello script, non avvia una richiesta HTTP e non controlla
    // se src usa HTTP o HTTPS. In questa versione autorizziamo solo script HTML;
    // gli script SVG hanno caratteristiche diverse e vengono rifiutati.
    if namespace != "http://www.w3.org/1999/xhtml" {
        return false;
    }

    // attr("src") restituisce Some(valore) se l'attributo esiste, None se manca.
    // Anche src="" entra nel ramo Some: un URL vuoto o invalido non deve
    // ripiegare sull'allow-list inline e ottenere un'autorizzazione diversa.
    match element.value().attr("src") {
        // Script esterno: config.rs verifica l'URL contro allowed_sources.
        Some(source) => policy.allows_source(source),
        // Script inline: text() visita i frammenti testuali; collect li unisce
        // in una String. config.rs la confronta esattamente con allowed_inline.
        None => policy.allows_inline(&element.text().collect::<String>()),
    }
}

/// Riconosce i nomi degli attributi che iniziano con on, ignorando il caso.
/// È usata da remove_event_handlers sia per selezionare gli elementi interessati
/// sia per decidere quali attributi eliminare. Non modifica il DOM.
/// Per esempio onclick e ONLOAD corrispondono, class e data-onclick no.
fn is_event_handler(name: &str) -> bool {
    // "on" occupa due byte ASCII. get(..2) prova a leggere quel prefisso senza
    // panic: se la stringa è troppo corta o il limite spezza UTF-8, dà None.
    name.get(..2)
        // Con Some verifica il prefisso; con None restituisce direttamente false.
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
}

/// Costruisce un percorso nel DOM con indici tra fratelli dello stesso tipo.
///
/// Riceve l'elemento interessato e restituisce una stringa per il campo location
/// del report, per esempio /html[1]/body[1]/button[2]. Non modifica il documento.
/// È usata dalle regole di rimozione. L'eventuale suffisso /@onclick
/// viene aggiunto da remove_event_handlers, non da questa funzione.
fn element_location(element: ElementRef<'_>) -> String {
    // Le parti vengono raccolte dal basso verso l'alto, partendo dall'elemento.
    let mut parts = Vec::new();

    // ancestors risale dal genitore verso la radice e non include l'elemento
    // iniziale. filter_map conserva solo gli antenati che sono elementi.
    // Così attraversiamo anche il nodo Fragment interno ai template.
    let ancestors = element.ancestors().filter_map(ElementRef::wrap);
    // once aggiunge l'elemento iniziale; chain prosegue con i suoi antenati.
    for element in std::iter::once(element).chain(ancestors) {
        // L'indice conta solo i fratelli precedenti con lo stesso nome
        // qualificato. Tra due button, un p o un commento non conta.
        let index = element
            .prev_siblings()
            .filter_map(ElementRef::wrap)
            .filter(|sibling| sibling.value().name == element.value().name)
            .count()
            // Gli indici partono da 1: nessun fratello uguale prima => indice 1.
            + 1;

        // Esempio della raccolta: button[2], poi body[1], poi html[1].
        parts.push(format!("{}[{index}]", element.value().name()));
    }

    // Invertiamo le parti per passare dall'ordine elemento->radice a radice->elemento.
    parts.reverse();
    // Uniamo con / e aggiungiamo lo / iniziale, ottenendo il percorso completo.
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use scraper::Selector;

    use super::{remove_disallowed_scripts, remove_event_handlers, remove_meta_refresh};
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

    #[test]
    fn removes_reload_and_redirect_meta_tags_and_reports_original_positions() {
        let input = concat!(
            "<!DOCTYPE html><html><head><meta charset=\"UTF-8\">",
            "<meta http-equiv=\"refresh\" content=\"5\">",
            "<meta name=\"description\" content=\"Pagina di prova\">",
            "<meta http-equiv=\"refresh\" content=\"0; url=https://untrusted.example\">",
            "</head><body><p>Da conservare</p></body></html>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_meta_refresh(&mut document);

        assert_eq!(
            document.html(),
            concat!(
                "<!DOCTYPE html><html><head><meta charset=\"UTF-8\">",
                "<meta content=\"Pagina di prova\" name=\"description\">",
                "</head><body><p>Da conservare</p></body></html>"
            )
        );
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].location, "/html[1]/head[1]/meta[2]");
        assert_eq!(actions[1].location, "/html[1]/head[1]/meta[4]");
        assert_eq!(
            actions[0].original_fragment,
            "<meta content=\"5\" http-equiv=\"refresh\">"
        );
        assert_eq!(
            actions[1].original_fragment,
            "<meta content=\"0; url=https://untrusted.example\" http-equiv=\"refresh\">"
        );
        assert!(actions
            .iter()
            .all(|action| action.rule == "remove-meta-refresh" && action.replacement.is_empty()));

        // Una seconda applicazione non deve produrre ulteriori modifiche.
        let output = serialize_html(&document);
        assert!(remove_meta_refresh(&mut document).is_empty());
        assert_eq!(serialize_html(&document), output);
    }

    #[test]
    fn recognizes_refresh_with_mixed_case_ascii_whitespace_and_html_entities() {
        for value in [
            "REFRESH",
            "ReFrEsH",
            " refresh ",
            "\trefresh\r\n",
            "r&#101;fr&#x65;sh",
        ] {
            let input = format!("<META HTTP-EQUIV=\"{value}\" content=\"0; url=/next\">");
            let mut document = parse_html(input.as_bytes()).unwrap();

            let actions = remove_meta_refresh(&mut document);

            assert_eq!(actions.len(), 1, "http-equiv: {value:?}");
            assert_eq!(document.html(), "<html><head></head><body></body></html>");
        }
    }

    #[test]
    fn removes_refresh_even_with_missing_or_invalid_content() {
        for input in [
            "<meta http-equiv=\"refresh\">",
            "<meta http-equiv=\"refresh\" content=\"\">",
            "<meta http-equiv=\"refresh\" content=\"not a delay\">",
        ] {
            let mut document = parse_html(input.as_bytes()).unwrap();

            let actions = remove_meta_refresh(&mut document);

            assert_eq!(actions.len(), 1, "input: {input}");
            assert_eq!(document.html(), "<html><head></head><body></body></html>");
        }
    }

    #[test]
    fn preserves_other_meta_tags_and_refresh_text_outside_meta_directives() {
        let input = concat!(
            "<head><meta charset=\"UTF-8\">",
            "<meta name=\"refresh\" content=\"0\">",
            "<meta http-equiv=\"content-type\" content=\"text/html\">",
            "<meta http-equiv=\"refreshing\" content=\"0\">",
            "<meta http-equiv=\"\" content=\"0\">",
            "<script>const example = '<meta http-equiv=\"refresh\" content=\"0\">';</script>",
            "</head><body><div http-equiv=\"refresh\">Testo</div>",
            "<!-- <meta http-equiv=\"refresh\" content=\"0\"> --></body>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();
        let before = serialize_html(&document);

        let actions = remove_meta_refresh(&mut document);

        assert!(actions.is_empty());
        assert_eq!(serialize_html(&document), before);
    }

    #[test]
    fn removes_refresh_inside_template_and_reports_its_location() {
        let input = concat!(
            "<main><template><meta http-equiv=\"refresh\" content=\"0; url=/next\">",
            "<p>Contenuto del template</p></template></main>"
        );
        let mut document = parse_html(input.as_bytes()).unwrap();

        let actions = remove_meta_refresh(&mut document);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].location,
            "/html[1]/body[1]/main[1]/template[1]/meta[1]"
        );
        assert!(document
            .html()
            .contains("<template><p>Contenuto del template</p></template>"));
    }
}
