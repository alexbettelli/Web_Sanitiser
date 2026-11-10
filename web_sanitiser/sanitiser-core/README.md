# sanitiser-core — Appunti di sviluppo

Resoconto del lavoro del **10 novembre 2026**: parsing e serializzazione HTML,
rimozione degli event handler inline, gestione degli script tramite allow-list.

Questo documento spiega soprattutto **il flusso dei dati**: da dove arrivano,
quale funzione li riceve, come li trasforma e a chi restituisce il risultato.
Descrive lo stato attuale del codice, distinguendolo dai collegamenti ancora
da realizzare. I requisiti generali rimangono nel
[README principale](../../README.md) e nel [PDF](../../final_project.pdf).

## 1. Mappa del codice e stato attuale

| File | Responsabilità | Stato |
|---|---|---|
| [src/input/local.rs](src/input/local.rs) | Leggere i byte di un file e individuare file locali | Già presente prima del lavoro sul parser |
| [src/parser/html.rs](src/parser/html.rs) | Convertire byte HTML in DOM e DOM in byte HTML | Implementato e testato |
| [src/sanitizer/html.rs](src/sanitizer/html.rs) | Rimuovere event handler e script non autorizzati | Implementato e testato |
| [src/config.rs](src/config.rs) | Descrivere la policy degli script e leggerla da testo TOML | Implementato e testato |
| [src/report.rs](src/report.rs) | Definire report e azioni serializzabili in JSON | Modello già presente, riutilizzato dalle regole |
| [src/sanitizer/engine.rs](src/sanitizer/engine.rs) | Coordinare la pipeline | Ancora vuoto |
| [src/lib.rs](src/lib.rs) | Esporre i moduli e l'ingresso pubblico `sanitise` | Moduli esposti; `sanitise` contiene ancora `todo!()` |
| [CLI](../sanitiser-cli/src/main.rs) | Ricevere gli argomenti dell'utente e coordinare l'esecuzione | Riconosce ed elenca gli input; non chiama ancora le regole |

Le singole funzioni del core possono già essere chiamate e combinate da codice
Rust. Il programma da terminale non esegue ancora la pipeline completa.
Chiamare oggi `sanitise` provoca il panic del suo `todo!()`.

Il flusso completo da collegare sarà:

```text
Utente
  │ indica input e configurazione
  ▼
CLI / programma che usa la libreria
  ├── legge il file HTML ───────────────────────► Vec<u8>
  └── legge il file TOML ─► Policy::from_toml ──► Policy
                                                  │
Vec<u8> ─► parse_html ─► DOM                        │
                         │                        │
                         ▼                        │
                remove_event_handlers             │
                         │                        │
                         ▼                        ▼
                remove_disallowed_scripts ◄── policy.scripts
                         │
                         ├── DOM modificato ─► serialize_html ─► byte HTML
                         └── azioni ──────────────────────────► report

CLI / livello di output: salvataggio dell'HTML e del report JSON
```

I blocchi del parser, delle regole e della policy sono implementati. Il
coordinamento nel motore, il caricamento dalla CLI e il salvataggio degli output
sono ancora da collegare.

## 2. Parsing: dai byte HTML al DOM

### Cosa fa l'utente e da dove arrivano i dati

L'utente sceglie un file, per esempio `pagina.html`. La stringa `"pagina.html"`
è il **percorso**, non il contenuto del documento.

Il file sul disco contiene byte. La funzione già presente
`input::read_file(path)` usa `std::fs::read` e restituisce `Vec<u8>`.
Non occorre prima creare una stringa e convertirla in byte.

```text
Percorso scelto dall'utente
         │
         ▼
input::read_file(path)
         │ restituisce i byte del contenuto
         ▼
Vec<u8>
         │ il chiamante passa &bytes
         ▼
parse_html(input: &[u8])
```

Questo passaggio tra lettura e parsing è disponibile come insieme di API;
non è ancora eseguito dalla CLI attuale.

### Cosa contiene la funzione

```rust
pub fn parse_html(input: &[u8]) -> Result<Html, Utf8Error> {
    let html = std::str::from_utf8(input)?;

    Ok(Html::parse_document(html))
}
```

Il primo passaggio verifica che i byte rappresentino testo UTF-8. Se la
verifica riesce, `from_utf8` restituisce `&str`, una vista testuale sugli stessi
byte, senza creare una `String` intermedia.

Il secondo passaggio costruisce l'albero del documento. Usiamo `scraper::Html`:
`scraper` impiega già **html5ever** come parser HTML5 e fornisce il DOM su cui
lavorano le nostre regole. Non abbiamo scritto un parser HTML da zero.

```text
&[u8]
  │ std::str::from_utf8
  ├── byte UTF-8 non validi ─► Err(Utf8Error) al chiamante
  └── testo valido: &str
           │ Html::parse_document, tramite html5ever
           ▼
       Ok(DOM)
           │
           ▼
   chiamante / future regole
```
L'errore di codifica e un errore nella struttura HTML sono due cose diverse.
Il parser HTML5 può recuperare molti documenti malformati, per esempio
ricostruendo chiusure mancanti. La nostra funzione restituisce un errore per
UTF-8 non valido; non rifiuta automaticamente ogni errore strutturale HTML.
Altre codifiche non sono ancora supportate.

### Cos'è il DOM restituito

Per questo frammento:

```html
<button onclick="evil()">Premi</button>
```

il DOM contiene un elemento `button`, il suo attributo `onclick` e un nodo
testuale con `Premi`, dentro la struttura del documento HTML.

Il parsing **non rimuove** l'attributo: costruisce la struttura su cui agirà
la regola successiva. Nel contesto del progetto è più preciso chiamare questa
operazione *parsing*; il termine *deserializzazione* aiuta a ricordare la
direzione testo → struttura.

## 3. Serializzazione: dal DOM ai byte HTML

### Da chi riceve il documento e a chi restituisce l'output

Il chiamante conserva il DOM restituito dal parser e lo passa alle regole.
Dopo le modifiche, passa quel DOM a `serialize_html`.

```text
DOM costruito dal parser
         │ modifiche delle regole
         ▼
DOM attuale
         │ &document
         ▼
serialize_html
         │ document.html()
         ▼
String contenente HTML
         │ into_bytes()
         ▼
Vec<u8> restituito al chiamante
         │
         ▼
Futuro livello di output: scrittura su file o altro utilizzo
```

La funzione è:

```rust
pub fn serialize_html(document: &Html) -> Vec<u8> {
    document.html().into_bytes()
}
```

`&Html` permette di leggere il DOM senza consumarlo o modificarlo.
`document.html()` crea il testo corrispondente all'albero corrente.
`into_bytes()` consuma quella `String` e ne restituisce i byte senza copiarne
il contenuto in un secondo buffer.

Le API usate restituiscono direttamente `String` e `Vec<u8>`: qui non c'è
un errore di decodifica da propagare tramite `Result`.

La serializzazione non conserva necessariamente spaziatura, virgolette e
tag impliciti del file originale. Può comparire una struttura esplicita con
`html`, `head` e `body`. È l'albero corrente a determinare l'output.

Questa funzione non salva file e non rimuove contenuti per conto proprio.
Anche un DOM non ancora sanitizzato può essere serializzato.

## 4. Event handler inline: onclick, onload, onerror e simili

### Input e risultato della regola

```rust
pub fn remove_event_handlers(document: &mut Html) -> Vec<SanitiseAction>
```

Il chiamante passa il DOM come `&mut Html`: la funzione prende in prestito
l'albero con il permesso di modificarlo. Non riceve percorsi o testo HTML.
Restituisce un elenco di azioni; il DOM viene modificato direttamente.

```text
Chiamante con un DOM
         │ &mut document
         ▼
remove_event_handlers
         ├── modifica gli attributi nel DOM ricevuto
         └── restituisce Vec<SanitiseAction>
                    │
                    ▼
           il chiamante aggiorna il report

Il DOM modificato prosegue verso le altre regole e la serializzazione.
```

L'utente non deve elencare a mano ogni `onclick`: la regola li cerca nel DOM.
Attualmente la sua applicazione è decisa dal chiamante; non esiste ancora
un'opzione CLI per abilitarla o disabilitarla.

### Come individua e rimuove gli attributi

1. Visita gli elementi collegati all'albero del documento.
2. Individua gli attributi con nome locale che inizia con `on`, ignorando
   maiuscole e minuscole, tramite `is_event_handler`.
3. Raccoglie identificativi e posizioni degli elementi interessati.
4. Accede agli elementi in modo modificabile e usa `retain` sugli attributi.
5. Per ogni attributo rimosso produce una `SanitiseAction`.

La raccolta iniziale permette di separare la lettura dell'albero dalla sua
modifica. `retain` conserva gli attributi per cui la funzione restituisce
`true` ed elimina quelli per cui restituisce `false`.

È una regola conservativa basata sul prefisso `on`, non un catalogo dei soli
eventi standard. `data-onclick` non inizia con `on` e viene conservato.

Prima:

```html
<button class="primary" onclick="evil()">Premi</button>
```

Dopo, come frammento:

```html
<button class="primary">Premi</button>
```

Il bottone, il testo e gli altri attributi rimangono. Per un attributo rimosso
il report contiene, per esempio:

```json
{
  "rule": "remove-event-handler",
  "location": "/html[1]/body[1]/button[1]/@onclick",
  "original_fragment": "onclick=\"evil()\"",
  "replacement": ""
}
```

Il frammento è ricostruito dai valori del DOM, con le necessarie escape per
virgolette e `&`: non è una copia della grafia esatta presente nel file.
In questa regola una seconda applicazione non produce ulteriori rimozioni.

## 5. Script e allow-list

### Le due decisioni: riconoscere e autorizzare

```rust
pub fn remove_disallowed_scripts(
    document: &mut Html,
    policy: &ScriptPolicy,
) -> Vec<SanitiseAction>
```

La funzione riceve il DOM dal chiamante e la configurazione degli script da
`policy.scripts`. Usa due controlli distinti:

- `is_script` riconosce gli elementi con nome locale `script`;
- `is_allowed_script` decide se conservarli in base a namespace, presenza
  di `src` e allow-list.

```text
DOM + policy.scripts
         │
         ▼
Elemento script?
  ├── no ─► la regola lo lascia invariato
  └── sì
       │
       ├── namespace diverso da HTML, per esempio SVG ─► rimozione
       └── namespace HTML
            │
            ├── attributo src presente
            │      └── verifica allowed_sources
            └── attributo src assente
                   └── verifica allowed_inline

Autorizzato ─► resta nel DOM
Non autorizzato ─► elemento e contenuto scollegati dal DOM + azione restituita
```

Le posizioni e i frammenti vengono acquisiti prima delle rimozioni, evitando
che gli indici dei fratelli cambino mentre si costruisce il report.
`detach()` scollega il sottoalbero: il relativo script non comparirà più
nell'HTML serializzato. Il report usa la regola `remove-disallowed-script`,
il frammento dell'intero elemento e una sostituzione vuota.

### Da dove arriva la policy

La configurazione è definita in `config.rs`:

```rust
pub struct Policy {
    pub scripts: ScriptPolicy,
}

pub struct ScriptPolicy {
    pub allowed_sources: Vec<String>,
    pub allowed_inline: Vec<String>,
}
```

Il file [policy.example.toml](../policy.example.toml) mostra il formato:

```toml
[scripts]
allowed_sources = ["https://cdn.example/assets/app.js?v=1"]
allowed_inline = ["window.appReady = true;"]
```

Il percorso previsto per la configurazione è:

```text
Utente scrive il file TOML
         │
         ▼
Chiamante legge il file come testo
         │ &str
         ▼
Policy::from_toml(config_text)
         ├── errore di sintassi, tipo o campo sconosciuto ─► Err al chiamante
         └── configurazione valida ─► Policy
                                          │ &policy.scripts
                                          ▼
                              remove_disallowed_scripts
```

`Policy::from_toml` deserializza **testo già ricevuto**: non apre file.
I campi omessi assumono i valori predefiniti. `Policy::default()`, un TOML
vuoto o una sezione `[scripts]` senza liste producono liste vuote; quando
la regola viene applicata, tutti gli script vengono quindi rimossi.

### Script esterni: allowed_sources

Per `<script src="...">`, `allows_source` interpreta l'URL tramite la crate
`url` e confronta l'intero URL normalizzato con le voci autorizzate.
Non usa sottostringhe, prefissi, wildcard o la sola appartenenza a un dominio.

Con la configurazione dell'esempio:

| Riferimento | Esito |
|---|---|
| `https://cdn.example/assets/app.js?v=1` | Autorizzato |
| `HTTPS://CDN.EXAMPLE:443/assets/app.js?v=1` | Autorizzato dopo normalizzazione |
| `https://cdn.example/other.js?v=1` | Rimosso: percorso diverso |
| `https://cdn.example/assets/app.js?v=2` | Rimosso: query diversa |
| `https://cdn.example.evil.test/assets/app.js?v=1` | Rimosso: dominio diverso |
| `/assets/app.js` o `//cdn.example/assets/app.js?v=1` | Rimosso: manca un URL assoluto esplicito |
| `https:cdn.example/assets/app.js?v=1` | Rimosso: forma ambigua senza `://` |
| `javascript:...`, `data:...`, `file:...` | Rimosso: schema non supportato |
| URL con nome utente o password | Rimosso |

Le liste possono autorizzare solo URL validi HTTP/HTTPS nella forma supportata;
inserire un riferimento relativo o uno schema vietato nella lista non lo abilita.
Per i relativi servirà il contesto della pagina, oggi non passato a questa regola.

### Script inline: allowed_inline

Per gli script senza `src`, il contenuto testuale del DOM deve coincidere
esattamente con una voce della lista. Spazi e ritorni a capo fanno parte
del confronto dopo le normalizzazioni del parsing HTML.

Con `allowed_inline = ["window.appReady = true;"]`:

```html
<!-- Conservato: contenuto esatto -->
<script>window.appReady = true;</script>

<!-- Rimosso: codice aggiuntivo -->
<script>window.appReady = true; evil();</script>

<!-- Rimosso: spazio iniziale non presente nella voce autorizzata -->
<script> window.appReady = true;</script>
```

Un `src` presente ma vuoto o invalido non fa ripiegare sull'allow-list inline.
La regola tratta conservativamente tutti gli elementi `script`, senza
esenzioni automatiche in base a `type`. Gli script SVG vengono rimossi anche
se un loro URL o contenuto compare nelle liste per gli script HTML.

### Interazione con event handler e download

Un riferimento autorizzato non autorizza gli attributi evento dello stesso
elemento. Per questo l'esempio di pipeline applica anche
`remove_event_handlers`: un `<script src="URL_APPROVATO" onload="evil()">`
può conservare il riferimento e perdere `onload`.

Questa regola non scarica né esegue JavaScript e non analizza se il suo
contenuto sia maligno. La lista esprime una decisione esplicita di fiducia:
per gli esterni autorizza il riferimento, non certifica il contenuto remoto.

Nel PDF il download delle sub-risorse è opzionale e richiede limiti su
profondità, numero di richieste e byte totali. Rimane separato dal controllo
del DOM. Un tag conservato può essere caricato successivamente dal browser;
non viene scaricato dal sanitiser durante questa funzione.

## 6. Cosa può passare oggi l'utente dalla linea di comando

Eseguendo i comandi dalla directory `web_sanitiser`, gli esempi supportati per
la classificazione degli input sono:

```powershell
cargo run --package sanitiser-cli -- pagina.html
cargo run --package sanitiser-cli -- pagina1.html pagina2.html
cargo run --package sanitiser-cli -- ./sito/
cargo run --package sanitiser-cli -- urls.txt
cargo run --package sanitiser-cli -- https://example.org/
```

I percorsi locali devono esistere. `urls.txt` viene riconosciuto come lista
quando contiene URL HTTP/HTTPS validi, oltre a eventuali righe vuote o commenti
che iniziano con `#`.

Il **flusso effettivo della CLI attuale** termina qui:

```text
Argomenti testuali dell'utente
         │ clap: Args.inputs
         ▼
input::resolve_args
         │ Vec<Input>
         ▼
main: stampa file, directory o URL
         │ per le directory usa anche walk_directory
         ▼
Fine: nessuna chiamata al parser o alle regole
```

Il riconoscimento di un URL non comporta un download. La presenza di più
argomenti non comporta ancora sanitizzazione batch o elaborazione concorrente.

Non sono ancora implementati gli argomenti `--config`, `--output`, `--fetch`
o flag per inserire direttamente voci nell'allow-list. `--help` e `--version`
sono forniti da `clap`. Il file TOML di esempio non viene caricato automaticamente.
Passarlo oggi come semplice argomento posizionale non lo rende una policy.

**La lista di URL in ingresso e l'allow-list sono due dati diversi:**

| Dato | Significato | Destinatario |
|---|---|---|
| `urls.txt` | Pagine o risorse da elaborare, con URL HTTP/HTTPS | Risoluzione degli input |
| `allowed_sources` nel TOML | Riferimenti esterni degli script da conservare | Policy degli script |
| `allowed_inline` nel TOML | Contenuti inline degli script da conservare | Policy degli script |

Quando aggiungeremo il caricamento della configurazione alla CLI, l'utente
potrà indicare il percorso del TOML. La CLI ne leggerà il testo e passerà
la `Policy` al motore. Oggi questa composizione è possibile solo chiamando
le API da codice Rust, come nell'esempio seguente.

## 7. Come comporre le funzioni già disponibili

Questo esempio usa le API attuali, ma **non è ancora il corpo della funzione
pubblica `sanitise`**. Mostra il collegamento che dovrà fare il motore.

```rust
use std::error::Error;

use sanitiser_core::config::Policy;
use sanitiser_core::parser::html::{parse_html, serialize_html};
use sanitiser_core::report::SanitiseReport;
use sanitiser_core::sanitizer::html::{
    remove_disallowed_scripts, remove_event_handlers,
};

fn esempio_pipeline(
    input: &[u8],
    config_text: &str,
) -> Result<(Vec<u8>, SanitiseReport), Box<dyn Error>> {
    let policy = Policy::from_toml(config_text)?;
    let mut document = parse_html(input)?;
    let mut report = SanitiseReport::default();

    report.actions.extend(remove_event_handlers(&mut document));
    report.actions.extend(remove_disallowed_scripts(
        &mut document,
        &policy.scripts,
    ));

    let output = serialize_html(&document);
    Ok((output, report))
}
```

Il chiamante deve avere già letto i byte HTML e il testo della configurazione.
Riceve poi due risultati distinti:

- `Vec<u8>`: contenuto HTML prodotto dalle regole attualmente implementate;
- `SanitiseReport`: elenco delle azioni accumulate con `extend`.

`Box<dyn Error>` serve qui a semplificare l'esempio, propagando sia errori
TOML sia errori UTF-8. L'integrazione definitiva dovrà raccordarli al tipo
di errore pubblico del core.

Le regole restituiscono azioni Rust, non stringhe JSON. Il modello del report
deriva `serde::Serialize`, quindi il livello di output potrà convertirlo in
JSON con `serde_json` e scriverlo. Né il parser né le regole salvano file.
In questo stadio sono implementati solo i controlli descritti nel documento:
l'output non ha ancora attraversato tutte le verifiche richieste dal progetto.

## 8. Verifiche effettuate e punto di ripresa

L'ultima verifica del codice ha completato **17 test**, con esito positivo,
eseguendo dalla directory `web_sanitiser`:

```powershell
cargo test --workspace --offline --locked
```

`--offline` richiede che le dipendenze siano già disponibili nella cache.
I test coprono UTF-8 invalido, recupero di HTML malformato, serializzazione di
accenti ed entità, rimozione degli eventi e conservazione degli altri contenuti,
configurazioni TOML, comportamento predefinito, confronti esatti delle allow-list,
URL ingannevoli, script nei template e in SVG, correttezza delle azioni e
riapplicazione delle regole senza ulteriori rimozioni.

Alla ripresa restano da sviluppare il coordinamento in `engine.rs`, il
collegamento di `sanitise`, la gestione degli errori del parser e della policy
nel modello pubblico, il caricamento della configurazione nella CLI e il
salvataggio degli output. Le ulteriori regole HTML, come meta refresh,
iframe/object e URI pericolosi, rimangono da aggiungere. Il fetching delle
risorse costituisce un lavoro successivo e separato.
