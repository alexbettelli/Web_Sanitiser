# sanitiser-core — Guida alle funzioni implementate

Questa guida accompagna la lettura delle funzioni scritte per il parsing HTML,
la rimozione degli event handler, il controllo degli script e i meta refresh. Ogni sezione
indica il file da aprire e segue i dati dall'ingresso al risultato.

Il collegamento tra le funzioni è questo:

```text
byte HTML ──► parse_html ──► DOM modificabile
                               │
                               ├── remove_event_handlers ──────► azioni
                               │
testo TOML ──► Policy::from_toml │
                    │          │
                    └──────────┼── remove_disallowed_scripts ──► azioni
                               │
                               ├── remove_meta_refresh ────────► azioni
                               │
                               └── serialize_html ────────────► byte HTML
```

È la composizione prevista per il motore. Le funzioni sono disponibili;
`sanitise` in `src/lib.rs` contiene ancora `todo!()` e la CLI non le collega
ancora in un'esecuzione completa.

## 1. File `src/parser/html.rs`: costruire il DOM e produrre l'HTML

Apri [src/parser/html.rs](src/parser/html.rs). In questo file ci sono le due
conversioni: dai byte all'albero e dall'albero ai byte.

### `parse_html(input)`

```rust
pub fn parse_html(input: &[u8]) -> Result<Html, Utf8Error> {
    let html = std::str::from_utf8(input)?;

    Ok(Html::parse_document(html))
}
```

**Riceve:** i byte del contenuto HTML come `&[u8]`. Il chiamante può ottenerli
con `read_file(path)`, già presente in
[src/input/local.rs](src/input/local.rs). Il percorso `"pagina.html"` serve
alla lettura del file: alla nostra funzione arriva il suo contenuto.

La riga `std::str::from_utf8(input)?` verifica che i byte siano UTF-8 validi e
li rende leggibili come `&str`. Questa vista usa i byte ricevuti senza creare
una `String` intermedia. Il `?` restituisce subito `Err(Utf8Error)` al chiamante
se la verifica fallisce.

`Html::parse_document(html)` costruisce il DOM. È una funzione di `scraper`,
che usa `html5ever` come parser HTML5. Il parser può recuperare errori
strutturali, come alcuni tag di chiusura mancanti. Questo recupero è distinto
dalla verifica della codifica: la nostra funzione accetta solo UTF-8.

**Restituisce:** `Ok(Html)`, cioè il documento strutturato. Il chiamante lo
conserva in una variabile e lo passa alle regole di `src/sanitizer/html.rs`.
Gli attributi pericolosi sono ancora presenti: questa funzione costruisce
l'albero, sul quale lavoreranno le regole.

```text
read_file(path) → Vec<u8> → parse_html(&bytes) → Result<Html, Utf8Error>
                                                   │
                                                   └── DOM alle regole
```

### `serialize_html(document)`

Sempre in [src/parser/html.rs](src/parser/html.rs):

```rust
pub fn serialize_html(document: &Html) -> Vec<u8> {
    document.html().into_bytes()
}
```

**Riceve:** un riferimento al DOM, normalmente dopo le modifiche delle regole.
`&Html` consente di leggerlo senza modificarlo o consumarlo.

`document.html()` produce una `String` che rappresenta l'albero corrente.
`into_bytes()` consuma quella stringa e restituisce il suo contenuto come
`Vec<u8>`, senza una seconda copia dei byte. Le API usate restituiscono
questi valori direttamente: qui non c'è un errore di decodifica da propagare.

**Restituisce:** i byte HTML al chiamante. Sarà il livello di output a scriverli
su file. La formattazione può cambiare rispetto all'input: il parser può aver
aggiunto tag impliciti, e la serializzazione ricostruisce l'HTML dal DOM.

```text
DOM modificato → serialize_html(&document) → Vec<u8> → futuro salvataggio
```

## 2. File `src/config.rs`: costruire e interrogare la policy

Apri [src/config.rs](src/config.rs). La struttura `Policy` contiene il campo
`scripts: ScriptPolicy`. Quest'ultimo contiene due liste di stringhe:

- `allowed_sources`: URL completi degli script esterni autorizzati;
- `allowed_inline`: contenuti esatti degli script inline autorizzati.

Le funzioni di questo file leggono o interrogano la configurazione. Il DOM
viene modificato dalle funzioni di `src/sanitizer/html.rs`.

### `Policy::from_toml(input)`

```rust
pub fn from_toml(input: &str) -> Result<Self, toml::de::Error> {
    toml::from_str(input)
}
```

**Riceve:** testo TOML già letto dal chiamante. La funzione non apre il file.
Per esempio, il testo di [policy.example.toml](../policy.example.toml) è:

```toml
[scripts]
allowed_sources = ["https://cdn.example/assets/app.js?v=1"]
allowed_inline = ["window.appReady = true;"]
```

`toml::from_str` usa le definizioni delle strutture per costruire `Policy`.
Gli attributi `#[serde(default, deny_unknown_fields)]` permettono di usare i
valori predefiniti per i campi omessi e fanno rifiutare i campi sconosciuti.
Anche sintassi e tipi errati producono un errore.

**Restituisce:** `Policy` oppure un errore TOML. Il chiamante passerà
`&policy.scripts` a `remove_disallowed_scripts`.
`Policy::default()` e le configurazioni senza liste producono liste vuote:
quando applicheremo la regola, tutti gli script saranno rimossi.

```text
utente scrive il TOML → chiamante legge il testo → Policy::from_toml
                                                         │
                                                         └── Policy
                                                               │
                                                               └── &policy.scripts alla regola
```

### `ScriptPolicy::allows_source(source)`

Sempre in [src/config.rs](src/config.rs):

```rust
pub fn allows_source(&self, source: &str) -> bool
```

**Riceve:** un valore di `src` estratto da un elemento script. `&self` è la
policy che contiene gli URL autorizzati.

La funzione chiama `parse_script_url(source)`. Se il riferimento non è
supportato, restituisce `false`. Altrimenti interpreta allo stesso modo le
voci di `allowed_sources` e confronta gli URL normalizzati per intero,
compresi percorso e query. Non usa confronti per prefisso o sottostringa.
Le voci della lista non interpretabili vengono ignorate durante il confronto.

**Restituisce:** `true` o `false` a `is_allowed_script`, definita in
`src/sanitizer/html.rs`. La decisione finale di rimozione spetta alla regola.

Con il TOML dell'esempio:

| `source` ricevuto | Risultato |
|---|---|
| `https://cdn.example/assets/app.js?v=1` | `true` |
| `HTTPS://CDN.EXAMPLE:443/assets/app.js?v=1` | `true`, dopo normalizzazione |
| `https://cdn.example/assets/app.js?v=2` | `false` |
| `https://cdn.example.evil.test/assets/app.js?v=1` | `false` |
| `/assets/app.js` | `false` |

### `parse_script_url(value)`

Questa funzione privata, sempre in [src/config.rs](src/config.rs), è chiamata
soltanto da `allows_source`:

```rust
fn parse_script_url(value: &str) -> Option<Url>
```

**Riceve:** una stringa proveniente da `src` oppure da `allowed_sources`.
Richiede la presenza di `://`, prova a interpretare l'URL con `Url::parse`
e accetta solo HTTP/HTTPS senza nome utente o password.

**Restituisce:** `Some(Url)` se il riferimento soddisfa questi controlli,
altrimenti `None`. Il `?` applicato a `Option` interrompe la funzione
restituendo `None` quando manca un dato necessario.

Perciò riferimenti relativi, forme come `https:app.js`, URL con credenziali e
schemi come `javascript:`, `data:` o `file:` vengono rifiutati anche se
inseriti nella lista. Per supportare i relativi servirà il contesto della pagina.

### `ScriptPolicy::allows_inline(content)`

La funzione in [src/config.rs](src/config.rs) esegue questo confronto:

```rust
self.allowed_inline.iter().any(|allowed| allowed == content)
```

**Riceve:** il testo dello script ricavato dal DOM. **Restituisce:** `true`
solo se coincide con una voce di `allowed_inline`, altrimenti `false`.
Spazi e ritorni a capo contano come parte del testo dopo le normalizzazioni
compiute dal parser HTML.

Con la voce `"window.appReady = true;"`, il contenuto esatto è autorizzato;
`" window.appReady = true;"` e `"window.appReady = true; evil();"` sono rifiutati.
Il risultato torna a `is_allowed_script` in `src/sanitizer/html.rs`.

## 3. File `src/sanitizer/html.rs`: rimuovere gli event handler

Apri [src/sanitizer/html.rs](src/sanitizer/html.rs). Qui lavoriamo sul DOM
costruito da `parse_html`, usando il modello di azione già definito in
[src/report.rs](src/report.rs).

### `remove_event_handlers(document)`

```rust
pub fn remove_event_handlers(document: &mut Html) -> Vec<SanitiseAction>
```

**Riceve:** il DOM come riferimento modificabile. Il chiamante conserva la
proprietà del documento; la funzione ne modifica direttamente gli attributi.

Il corpo della funzione segue questi passaggi:

1. `descendants()` visita i nodi del documento; `ElementRef::wrap` seleziona
   quelli che rappresentano elementi.
2. `is_event_handler` controlla i nomi degli attributi. Per gli elementi
   interessati, la funzione raccoglie l'identificativo e la posizione
   calcolata da `element_location`.
3. `get_mut(node_id)` permette di modificare l'elemento individuato.
4. `element.attrs.retain(...)` decide quali attributi conservare: `true`
   mantiene l'attributo; `false` lo elimina.
5. Prima di eliminarlo, la funzione aggiunge una `SanitiseAction` ad `actions`.

La raccolta iniziale separa la lettura dell'albero dalla sua modifica.
Per un bottone con `class="primary"` e `onclick="evil()"`, scompare soltanto
`onclick`: il bottone, il testo e `class` restano nel DOM.

**Restituisce:** un vettore di azioni al chiamante, che potrà aggiungerlo a
`report.actions`. Il DOM modificato rimane disponibile per la regola sugli
script e per `serialize_html`.

```text
remove_event_handlers                         [src/sanitizer/html.rs]
  ├── is_event_handler(nome) → attributo da rimuovere?
  ├── element_location(elemento) → posizione per il report
  ├── retain → modifica gli attributi del DOM
  └── Vec<SanitiseAction> → chiamante → report.actions
```

### `is_event_handler(name)`

È una funzione privata dello stesso file:

```rust
fn is_event_handler(name: &str) -> bool {
    name.get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
}
```

**Riceve:** il nome locale di un attributo. **Restituisce:** `true` se i primi
due caratteri sono `on`, ignorando maiuscole e minuscole.
Per esempio `onclick`, `onload`, `onerror` e `ONFOCUS` corrispondono;
`class`, `src` e `data-onclick` non corrispondono.

È una regola conservativa sul prefisso, non un elenco dei soli eventi standard.
La funzione riconosce il nome; è `remove_event_handlers` a eliminare l'attributo.

## 4. File `src/sanitizer/html.rs`: rimuovere gli script non autorizzati

Restiamo in [src/sanitizer/html.rs](src/sanitizer/html.rs). Questa regola
riceve anche la policy preparata dalle funzioni di `src/config.rs`.

### `remove_disallowed_scripts(document, policy)`

```rust
pub fn remove_disallowed_scripts(
    document: &mut Html,
    policy: &ScriptPolicy,
) -> Vec<SanitiseAction>
```

**Riceve:** il DOM modificabile e la policy degli script in sola lettura.

La visita iniziale individua gli elementi che soddisfano questa condizione:

```rust
is_script(*element) && !is_allowed_script(*element, policy)
```

Per ogni elemento da rimuovere prepara una `SanitiseAction` con il frammento
HTML e la posizione originale. Questa preparazione avviene prima delle
rimozioni, così gli indici dei fratelli rimangono quelli iniziali.

Nel secondo ciclo, `node.detach()` scollega l'elemento e il suo contenuto
dall'albero del documento: lo script non comparirà nell'HTML serializzato.
Gli elementi autorizzati rimangono collegati al DOM.

**Restituisce:** le azioni di rimozione al chiamante. Anche qui il documento
viene modificato direttamente, senza restituire un nuovo DOM.

```text
remove_disallowed_scripts                     [src/sanitizer/html.rs]
  ├── is_script → è un elemento script?
  ├── is_allowed_script → la policy lo autorizza?
  │     ├── allows_source → parse_script_url   [src/config.rs]
  │     └── allows_inline                     [src/config.rs]
  ├── element_location + element.html() → dati dell'azione
  ├── detach → scollega lo script dal DOM
  └── Vec<SanitiseAction> → chiamante → report.actions
```

### `is_script(element)`

Questa funzione privata di [src/sanitizer/html.rs](src/sanitizer/html.rs)
confronta il nome locale dell'elemento con `"script"`, ignorando il caso.
Riceve un `ElementRef` e restituisce un `bool`. Il riconoscimento comprende
anche gli script in namespace diversi da HTML; l'autorizzazione viene
valutata dalla funzione successiva.

### `is_allowed_script(element, policy)`

Sempre in [src/sanitizer/html.rs](src/sanitizer/html.rs), la funzione riceve
lo script e la policy e restituisce un `bool`.

Per prima cosa verifica il namespace dell'elemento: nella versione attuale
soltanto gli script HTML possono essere autorizzati. Gli script SVG vengono
rimossi. Per gli script HTML, il passaggio centrale è:

```rust
match element.value().attr("src") {
    Some(source) => policy.allows_source(source),
    None => policy.allows_inline(&element.text().collect::<String>()),
}
```

Se esiste `src`, il suo valore viene passato ad `allows_source` in
`src/config.rs`. Se manca, il testo dello script viene raccolto in una
`String` e passato ad `allows_inline`, nello stesso file della policy.

Un `src=""` entra comunque nel ramo `Some`: il riferimento viene rifiutato
senza provare l'allow-list inline. Il risultato torna a
`remove_disallowed_scripts`, che rimuove lo script quando riceve `false`.
Non ci sono esenzioni automatiche in base all'attributo `type`.

Autorizzare uno script non autorizza gli event handler sul suo elemento:
per rimuovere un eventuale `onload` dobbiamo applicare anche
`remove_event_handlers`. Nessuna di queste funzioni scarica o esegue JavaScript.
L'allow-list degli esterni autorizza il riferimento, non verifica il suo
contenuto remoto; il fetching rimane un modulo successivo.

## 5. File `src/sanitizer/html.rs`: rimuovere i meta refresh

Restiamo in [src/sanitizer/html.rs](src/sanitizer/html.rs). Un meta refresh
può chiedere al browser di ricaricare la pagina o aprire un altro URL.
La regola attuale rimuove tutti questi elementi quando il chiamante la applica.

### `remove_meta_refresh(document)`

```rust
pub fn remove_meta_refresh(document: &mut Html) -> Vec<SanitiseAction>
```

**Riceve:** il DOM modificabile. Visita gli elementi e usa `is_meta_refresh`
per selezionare quelli da rimuovere. Prima raccoglie ID, posizione e frammento
HTML; poi chiama `detach()` per scollegare ciascun meta dal documento.

Per esempio, elimina `<meta http-equiv="refresh" content="5">` e
`<meta http-equiv="refresh" content="0; url=https://example.org">`, conservando
meta come `<meta charset="UTF-8">` e il resto della pagina.

**Restituisce:** le azioni con regola `remove-meta-refresh`, posizione nel DOM,
frammento precedente alla rimozione e sostituzione vuota. Il chiamante le
aggiunge al report e passa il DOM modificato alla serializzazione.
La funzione non interpreta il ritardo o l'URL in `content`: rimuove anche
i meta refresh con `content` assente, vuoto o non valido.

### `is_meta_refresh(element)`

Questa funzione privata dello stesso file riceve una vista dell'elemento
e restituisce un `bool`. Verifica che il namespace sia HTML e il nome sia
`meta`, poi legge `http-equiv`. Se l'attributo manca, restituisce `false`;
altrimenti confronta il valore con `refresh`, ignorando il caso e gli spazi
ASCII esterni. Il parser ha già decodificato eventuali entità HTML.

Un attributo `http-equiv="refresh"` su un `div` non ne provoca la rimozione.
Anche una stringa con quel testo dentro uno script o un commento rimane
invariata: cerchiamo elementi nel DOM, non sottostringhe nel sorgente.

```text
remove_meta_refresh                           [src/sanitizer/html.rs]
  ├── is_meta_refresh → è un meta HTML con http-equiv refresh?
  ├── element_location + element.html() → dati dell'azione
  ├── detach → scollega il meta dal documento
  └── Vec<SanitiseAction> → chiamante → report.actions
```

## 6. File `src/sanitizer/html.rs`: descrivere la posizione delle modifiche

### `element_location(element)`

Questa funzione privata di [src/sanitizer/html.rs](src/sanitizer/html.rs)
è condivisa dalle regole di rimozione:

```rust
fn element_location(element: ElementRef<'_>) -> String
```

**Riceve:** un elemento del DOM. Risale ai suoi antenati e, per ciascun
livello, conta i fratelli precedenti con lo stesso nome qualificato.
Costruisce le parti del percorso, le inverte e le unisce con `/`.
I nodi intermedi che non sono elementi, come il frammento di un template,
vengono attraversati mentre si cercano gli antenati.

**Restituisce:** un percorso come
`/html[1]/body[1]/main[1]/button[2]`. L'indice distingue i bottoni ripetuti.
`remove_event_handlers` aggiunge il nome dell'attributo, per esempio
`/@onclick`; la regola sugli script usa la posizione dell'intero elemento.

Le azioni sono definite in [src/report.rs](src/report.rs):

| Campo di `SanitiseAction` | Valore prodotto dalle regole |
|---|---|
| `rule` | `remove-event-handler`, `remove-disallowed-script` o `remove-meta-refresh` |
| `location` | Percorso calcolato da `element_location`, con eventuale attributo |
| `original_fragment` | Attributo ricostruito oppure HTML dello script o del meta prima della rimozione |
| `replacement` | Stringa vuota, perché la modifica è una rimozione |

Il frammento deriva dal DOM, quindi può differire dalla formattazione originale.
Le funzioni restituiscono strutture Rust; il livello di output potrà convertirle
in JSON usando il modello serializzabile già presente in `src/report.rs`.

## 7. Collegare le funzioni: un esempio di chiamante

Questo esempio combina le API descritte. È il collegamento da trasferire nel
motore: [src/sanitizer/engine.rs](src/sanitizer/engine.rs) è ancora vuoto e
`sanitise` in [src/lib.rs](src/lib.rs) contiene ancora `todo!()`.

```rust
use std::error::Error;
use sanitiser_core::config::Policy;
use sanitiser_core::parser::html::{parse_html, serialize_html};
use sanitiser_core::report::SanitiseReport;
use sanitiser_core::sanitizer::html::{
    remove_disallowed_scripts, remove_event_handlers, remove_meta_refresh,
};

fn esempio_pipeline(
    input: &[u8],
    config_text: &str,
) -> Result<(Vec<u8>, SanitiseReport), Box<dyn Error>> {
    let policy = Policy::from_toml(config_text)?;       // src/config.rs
    let mut document = parse_html(input)?;            // src/parser/html.rs
    let mut report = SanitiseReport::default();       // src/report.rs

    report.actions.extend(
        remove_event_handlers(&mut document),         // src/sanitizer/html.rs
    );
    report.actions.extend(
        remove_disallowed_scripts(&mut document, &policy.scripts),
    );
    report.actions.extend(
        remove_meta_refresh(&mut document),           // src/sanitizer/html.rs
    );

    let output = serialize_html(&document);           // src/parser/html.rs
    Ok((output, report))
}
```

Il chiamante riceve il contenuto HTML elaborato e il report. Dovrà poi salvarli.
`Box<dyn Error>` semplifica l'esempio; il motore definitivo dovrà raccordare
gli errori TOML e UTF-8 con il tipo di errore pubblico del core.

### Da dove entreranno gli argomenti dell'utente

Nel file [sanitiser-cli/src/main.rs](../sanitiser-cli/src/main.rs), `main`
riceve gli input da `Args::parse()` e li passa a `input::resolve_args`.
Oggi stampa gli input riconosciuti e, per le directory, elenca i file.
Il collegamento alle funzioni sopra deve ancora essere implementato.

La CLI accetta percorsi di file, directory, liste di URL e URL HTTP/HTTPS,
anche come più argomenti. Non sono ancora presenti `--config`, `--output`
o `--fetch`. Il riconoscimento di un URL non avvia un download.

Quando aggiungeremo il caricamento della configurazione, il flusso sarà:

```text
percorso HTML dalla CLI → read_file → byte per parse_html
percorso TOML dalla CLI → lettura del testo → Policy::from_toml
```

`urls.txt` identifica gli input da elaborare; `allowed_sources` nel TOML
identifica gli script da autorizzare. Passare il TOML come semplice argomento
posizionale oggi non lo carica come policy.

I test delle funzioni si trovano nei blocchi `#[cfg(test)]` di
`src/parser/html.rs`, `src/config.rs` e `src/sanitizer/html.rs`.
L'ultima verifica del codice ha completato 22 test con esito positivo.
Dalla directory `web_sanitiser` si possono eseguire con
`cargo test --workspace --offline --locked`, se le dipendenze sono in cache.
