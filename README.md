# Web_Sanitiser
## 1. Obiettivo generale

Il progetto consiste nella progettazione, implementazione e valutazione di un **web sanitiser scritto in Rust**.

Il sanitiser è uno strumento difensivo che riceve contenuti web potenzialmente pericolosi — HTML, risorse scaricate e asset incorporati — li analizza e **rimuove o modifica il materiale maligno o indesiderato** prima che possa raggiungere il sistema dell'utente.

Il progetto deve essere fornito in due forme:

- una **CLI (Command-Line Interface)** utilizzabile dal terminale;
- una **libreria Rust (crate)** riutilizzabile da altri programmi.

La distinzione fondamentale è:

```text
CLI = interfaccia utente
Core library = vero motore di sanitizzazione
```

La libreria deve essere indipendente dalla CLI, così da poter essere integrata in:

- download manager;
- mail client;
- proxy locali;
- scanner CI;
- altre pipeline di sistema.

---

## 2. I tre obiettivi principali

### 2.1 Pipeline modulare e configurabile

La sanitizzazione deve essere organizzata come una pipeline modulare.

Le policy devono essere:

- **dichiarative**;
- configurabili;
- non hard-coded nel codice.

Esempio concettuale:

```toml
[html]
allow_scripts = false
allow_iframes = false
remove_event_handlers = true

[urls]
block_javascript = true
block_data = true

[resources]
fetch = true
max_depth = 3
max_requests = 50
max_bytes = 100000000
```

L'idea è che il comportamento del sanitiser possa essere modificato cambiando la configurazione, senza modificare il codice.

---

### 2.2 Implementazione idiomatica in Rust

Il progetto deve essere scritto in Rust, sfruttando:

- memory safety;
- ownership/borrowing;
- type safety;
- ecosistema Rust;
- async/concurrency;
- possibilità di distribuire il lavoro tra più worker/core.

La concorrenza sarà particolarmente utile nella modalità batch e nel fetching delle risorse.

---

### 2.3 Valutazione funzionale e non funzionale

Il programma deve essere valutato sotto due aspetti.

#### Funzionale

Verificare se:

- riconosce correttamente le minacce;
- rimuove o riscrive ciò che deve essere rimosso;
- non modifica inutilmente contenuti leciti;
- applica correttamente le policy;
- genera report corretti.

#### Non funzionale

Misurare:

- **throughput** — quantità di dati processati per unità di tempo;
- **latency** — tempo necessario per elaborare un input;
- **memory footprint** — quantità di memoria utilizzata;
- **scalabilità** — comportamento aumentando il numero di worker/core.

---

# 3. Input richiesti

La CLI deve poter accettare uno o più input.

### File HTML/asset locali

```bash
web-sanitizer page.html
```

### Directory tree

```bash
web-sanitizer ./website/
```

La directory può contenere:

```text
website/
├── index.html
├── about.html
├── css/
├── images/
└── js/
```

### Lista di URL

```bash
web-sanitizer urls.txt
```

Esempio:

```text
https://example.com
https://example.org/page.html
```

È richiesto anche il **batch mode**, quindi:

```bash
web-sanitizer file1.html file2.html file3.html
```

deve essere possibile.

---

# 4. Output

Per ogni input il programma deve produrre:

1. **contenuto sanitizzato**;
2. **report JSON machine-readable**.

Esempio:

```text
page.html
    |
    v
sanitizer
    |
    +--> page.cleaned.html
    |
    +--> page.json
```

Il report deve essere strutturato e auditabile.

Esempio concettuale:

```json
{
  "input": "page.html",
  "status": "sanitized",
  "actions": [
    {
      "rule": "remove-event-handler",
      "location": "body/button",
      "original": "onclick="evil()"",
      "replacement": null
    }
  ]
}
```

Per ogni azione devono essere disponibili almeno:

- regola applicata;
- posizione nel documento;
- frammento originale;
- sostituzione effettuata.

Questo permette di ricostruire esattamente cosa ha fatto il sanitiser.

---

# 5. Controlli HTML obbligatori

L'HTML deve essere prima parsato in un **DOM** usando un parser HTML sicuro/esistente.

Non bisogna implementare un parser HTML da zero.

Il sanitiser deve gestire almeno:

### Event handler inline

Esempi:

```html
<button onclick="evil()">
<img onerror="evil()">
<form onsubmit="evil()">
```

Devono essere rimossi secondo policy.

### Script

Esempio:

```html
<script>
    ...
</script>
```

Gli script devono essere verificati rispetto a una allow-list/configurazione.

### URI pericolosi

Esempio:

```html
<a href="javascript:alert(1)">
```

Devono essere rilevati e gestiti.

Anche gli URI `data:` devono essere controllati secondo policy.

### iframe e object

Esempio:

```html
<iframe src="https://untrusted.example">
```

Il sanitiser deve verificare l'origine e gestire gli elementi non affidabili.

### Meta refresh

Esempio:

```html
<meta http-equiv="refresh"
      content="0;url=https://untrusted.example">
```

Deve essere rilevato e gestito.

---

# 6. URL e link inspection

Il programma deve estrarre gli URL presenti nel documento.

Non solo gli `<a>`, ma anche URL presenti in:

- anchor;
- form;
- immagini;
- script;
- CSS;
- iframe;
- altri riferimenti a risorse.

Esempi:

```html
<a href="...">
<form action="...">
<img src="...">
<script src="...">
<link href="...">
<iframe src="...">
```

Gli URL devono essere confrontati con una policy configurabile.

La policy può includere:

- malware domains;
- known trackers;
- pattern di IDN homograph;
- protocolli vietati;
- origini non fidate.

Quando viene trovato un URL sospetto, il comportamento dipende dalla policy:

```text
URL sospetto
    |
    +--> remove
    |
    +--> rewrite
    |
    +--> warning placeholder
```

---

# 7. Embedded resources

Il sanitiser deve poter, **opzionalmente**, scaricare le sub-risorse referenziate.

Esempio:

```text
index.html
    |
    +--> style.css
    +--> script.js
    +--> image.png
```

Le risorse possono essere sottoposte a sanitizzazione specifica per tipo:

```text
HTML       -> HTML sanitisation
CSS        -> CSS sanitisation
JavaScript -> JS sanitisation
Image      -> image/resource checks
```

Il fetching deve avere limiti rigidi per evitare che contenuti malevoli consumino risorse eccessive.

Devono essere configurabili almeno:

- profondità massima;
- numero massimo di richieste;
- numero massimo di byte totali.

Esempio:

```toml
[resources]
fetch = true
max_depth = 3
max_requests = 50
max_bytes = 100000000
```

---

# 8. CLI requirements

La CLI deve supportare:

### Batch mode

```bash
web-sanitizer file1.html file2.html file3.html
```

### Configuration file

```bash
web-sanitizer --config policy.toml page.html
```

### Output verbosity

Deve essere possibile scegliere quanto output mostrare nel terminale.

Esempio concettuale:

```text
quiet
normal
verbose
```

La verbosity riguarda principalmente l'output umano della CLI; il JSON rimane machine-readable.

### Exit code

Il programma deve restituire un **exit code non-zero** quando, secondo la policy, il contenuto deve essere rifiutato completamente.

Esempio concettuale:

```text
Clean       -> exit 0
Sanitized   -> exit 0
Rejected    -> exit != 0
Error       -> exit != 0
```

Questo è importante per l'integrazione in pipeline CI e altri programmi.

---

# 9. Architettura generale

La decomposizione consigliata è:

```text
                    CLI
                 main.rs
                    |
                    v
             sanitizer-core
                    |
       +------------+-------------+
       |            |             |
       v            v             v
    Input         Parser        Policy
       |            |             |
       +------------+-------------+
                    |
                    v
              Core Engine
                    |
       +------------+-------------+
       |            |             |
       v            v             v
      HTML         URLs       Resources
                    |
                    v
                Scheduler
              /     |      \
           Worker Worker Worker
              \     |      /
                    v
                 Reports
                 /      \
             HTML       JSON
```

---

# 10. Separazione CLI / library

Questa è una delle decisioni architetturali fondamentali.

Il progetto deve avere una **binary crate** per la CLI e una **library crate** per il motore.

```text
sanitizer-cli
      |
      | dipende da
      v
sanitizer-core
```

Il core non deve dipendere dalla CLI.

Questo permette di utilizzare il motore anche da:

```text
download manager
mail client
local proxy
CI scanner
```

senza dover eseguire il programma da terminale.

Inoltre permette di testare il core indipendentemente dalla CLI.

---

# 11. Struttura consigliata delle crate

```text
web-sanitizer/
│
├── Cargo.toml
├── Cargo.lock
│
├── crates/
│   ├── sanitizer-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       │
│   │       ├── input/
│   │       │   ├── mod.rs
│   │       │   ├── source.rs
│   │       │   ├── local.rs
│   │       │   └── remote.rs
│   │       │
│   │       ├── parser/
│   │       │   ├── mod.rs
│   │       │   └── html.rs
│   │       │
│   │       ├── policy/
│   │       │   ├── mod.rs
│   │       │   └── rules.rs
│   │       │
│   │       ├── sanitizer/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs
│   │       │   ├── html.rs
│   │       │   ├── urls.rs
│   │       │   └── resources.rs
│   │       │
│   │       ├── scheduler/
│   │       │   ├── mod.rs
│   │       │   └── pool.rs
│   │       │
│   │       └── report/
│   │           ├── mod.rs
│   │           └── model.rs
│   │
│   └── sanitizer-cli/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── cli.rs
│           ├── config.rs
│           └── output.rs
│
├── tests/
│   ├── fixtures/
│   └── integration/
│
└── benches/
    └── sanitization.rs
```

---

# 12. Responsabilità dei moduli

## `input/`

Si occupa di ottenere gli input.

### `source.rs`

Definisce i tipi di input:

```rust
pub enum Input {
    File(PathBuf),
    Directory(PathBuf),
    Url(url::Url),
}
```

### `local.rs`

Gestisce file e directory locali.

### `remote.rs`

Gestisce URL e HTTP fetching.

Deve inoltre gestire limiti come:

- timeout;
- response size;
- redirect;
- protocolli consentiti.

---

## `parser/`

Trasforma:

```text
HTML bytes/string
       |
       v
     DOM
```

### `html.rs`

Gestisce il parsing HTML.

Il parser deve essere una libreria esistente e sicura, non un parser scritto da zero.

---

## `policy/`

Contiene la configurazione del comportamento del sanitiser.

Esempio:

```rust
pub struct Policy {
    pub html: HtmlPolicy,
    pub urls: UrlPolicy,
    pub resources: ResourcePolicy,
}
```

La configurazione viene caricata dal file TOML.

---

## `sanitizer/`

È il cuore del progetto.

### `engine.rs`

Coordina l'intera pipeline.

### `html.rs`

Contiene le regole HTML:

- event handlers;
- script;
- iframe;
- object;
- meta refresh;
- altri elementi pericolosi.

### `urls.rs`

Controlla gli URL:

- `javascript:`;
- `data:`;
- blocklist;
- tracker;
- domini sospetti;
- IDN homograph.

### `resources.rs`

Gestisce il fetching e la sanitizzazione delle sub-risorse.

---

## `scheduler/`

Gestisce il batch processing e la concorrenza.

Concettualmente:

```text
1000 input
    |
    v
scheduler
    |
    +--> worker 1
    +--> worker 2
    +--> worker 3
    +--> worker 4
    |
    v
aggregated reports
```

Una prima implementazione può utilizzare Tokio e un limite sul numero di lavori concorrenti.

---

## `report/`

Definisce il modello dei report.

Esempio:

```rust
#[derive(Debug, Serialize)]
pub struct Report {
    pub input: String,
    pub actions: Vec<SanitizationAction>,
    pub status: SanitizationStatus,
}
```

e:

```rust
#[derive(Debug, Serialize)]
pub struct SanitizationAction {
    pub rule: String,
    pub location: String,
    pub original: String,
    pub replacement: Option<String>,
}
```

---

## `cli/`

La CLI si occupa di:

- parsing degli argomenti;
- caricamento della configurazione;
- output;
- verbosity;
- gestione exit code.

Non deve contenere la logica delle regole di sanitizzazione.

---

# 13. Dipendenze Rust consigliate

Lo stack iniziale può essere:

```text
html5ever
markup5ever_rcdom
reqwest
url
tokio
serde
serde_json
toml
clap
thiserror
anyhow
tracing
tracing-subscriber
regex
walkdir
criterion
tempfile
assert_cmd
predicates
pretty_assertions
```

## Ruolo principale

| Crate | Utilizzo |
|---|---|
| `html5ever` | parsing HTML5 |
| `markup5ever_rcdom` | rappresentazione DOM |
| `reqwest` | HTTP/fetch URL |
| `url` | parsing e normalizzazione URL |
| `tokio` | async/concurrency |
| `serde` | serializzazione/deserializzazione |
| `serde_json` | report JSON |
| `toml` | policy/config file |
| `clap` | CLI |
| `thiserror` | errori del core |
| `anyhow` | gestione errori applicativa nella CLI |
| `tracing` | logging |
| `tracing-subscriber` | configurazione del logging |
| `regex` | pattern matching |
| `walkdir` | scansione directory |
| `criterion` | benchmark |
| `tempfile` | test con file temporanei |
| `assert_cmd` | test della CLI |
| `predicates` | asserzioni sui risultati CLI |
| `pretty_assertions` | confronti leggibili nei test |

---

# 14. Esempio di utilizzo finale

L'obiettivo è arrivare a comandi del genere:

```bash
web-sanitizer page.html
```

```bash
web-sanitizer file1.html file2.html file3.html
```

```bash
web-sanitizer ./website/
```

```bash
web-sanitizer urls.txt
```

```bash
web-sanitizer     --config policy.toml     --verbosity verbose     --output ./clean     file1.html file2.html
```

Pipeline concettuale:

```text
Input
  |
  v
Input Layer
  |
  v
Parser
  |
  v
DOM
  |
  v
Policy
  |
  v
Sanitisation Engine
  |
  +--> HTML checks
  +--> URL checks
  +--> resource checks
  |
  v
Sanitised content
  |
  +--> output file
  |
  +--> JSON report
  |
  v
Exit code
```

---

# 16. Strategia di implementazione consigliata

Non conviene implementare tutto contemporaneamente.

### Fase 1 — Core minimo

Implementare:

- workspace;
- `sanitizer-core`;
- `sanitizer-cli`;
- input da file;
- parsing HTML;
- DOM;
- prime regole HTML;
- output sanitizzato;
- JSON report.

### Fase 2 — Policy

Aggiungere:

- `Policy`;
- file TOML;
- allow-list;
- block-list;
- regole configurabili.

### Fase 3 — URL inspection

Implementare:

- estrazione URL;
- `javascript:`;
- `data:`;
- blocklist;
- tracker;
- controlli IDN;
- rewrite/remove/warning.

### Fase 4 — Remote fetching

Aggiungere:

- URL input;
- `reqwest`;
- timeout;
- redirect limits;
- byte limits;
- request limits.

### Fase 5 — Embedded resources

Aggiungere:

- CSS;
- JS;
- immagini;
- depth limit;
- total byte limit;
- request limit.

### Fase 6 — Concorrenza

Aggiungere:

- batch scheduler;
- worker;
- Tokio;
- aggregazione report;
- configurazione del numero di worker.

### Fase 7 — CLI completa

Aggiungere:

- verbosity;
- output directory;
- exit codes;
- error handling;
- help/documentazione.

### Fase 8 — Test

Creare fixture:

```text
tests/fixtures/
├── clean.html
├── onclick.html
├── script.html
├── javascript-uri.html
├── iframe.html
├── meta-refresh.html
├── malicious-links.html
└── mixed.html
```

Testare:

```text
input → sanitiser → output + report
```

### Fase 9 — Benchmark

Misurare:

```text
1 worker
2 workers
4 workers
8 workers
16 workers
```

su input di dimensioni diverse.

Misurare:

- latency;
- throughput;
- memoria;
- scaling.

---

# 17. Cosa NON fare

### Non implementare un parser HTML da zero

Usare una libreria esistente.

### Non mettere la sanitizzazione in `main.rs`

La CLI deve chiamare il core.

### Non hard-codificare tutte le policy

Devono essere configurabili.

### Non fare fetching senza limiti

Una pagina non fidata può tentare di causare:

- troppe richieste;
- download enormi;
- profondità eccessiva;
- consumo di memoria/CPU.

### Non aggiungere troppe dipendenze prematuramente

Partire con lo stack minimo e aggiungere librerie solo quando servono.

---

# 18. Idea architetturale finale

Il progetto può essere riassunto così:

```text
                         USER
                          |
                          v
                 +----------------+
                 |      CLI       |
                 | sanitizer-cli  |
                 +-------+--------+
                         |
                         v
                 +----------------+
                 | SANITIZER CORE |
                 |  library crate |
                 +-------+--------+
                         |
          +--------------+--------------+
          |              |              |
          v              v              v
       INPUT          PARSER         POLICY
          |              |              |
          +--------------+--------------+
                         |
                         v
                 +----------------+
                 | SANITIZER      |
                 | ENGINE         |
                 +-------+--------+
                         |
              +----------+----------+
              |          |          |
              v          v          v
            HTML        URL      RESOURCES
              |          |          |
              +----------+----------+
                         |
                         v
                    SCHEDULER
                         |
                    +----+----+
                    |    |    |
                   W1   W2   W3
                    |    |    |
                    +----+----+
                         |
                         v
                      REPORT
                     /      \
                  HTML       JSON
```
