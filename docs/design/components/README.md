# Blocchi della lezione (catalogo SDUI v1)

> Specifiche visive, di interazione e didattiche dei 17 blocchi di `docs/contracts/sdui.md` (§4). Destinatari: `android-dev` (renderer), `llm-engineer` (Generator, sezione "Linee guida per il Generator" qui sotto e in fondo a ogni file). Valori in `../tokens.json`; componenti base in `../base-components.md`; flusso e layout in `../screens/lesson.md`. I principi P1-P12 sono in `../design-system.md` §1.

## Indice

| Blocco | File | Famiglia |
|---|---|---|
| `heading` | [heading.md](heading.md) | struttura |
| `paragraph` | [paragraph.md](paragraph.md) (include il **markdown inline** comune) | testo |
| `callout` | [callout.md](callout.md) | testo |
| `code` | [code.md](code.md) | testo |
| `diagram` | [diagram.md](diagram.md) | figura |
| `glossary_term` | [glossary_term.md](glossary_term.md) | testo |
| `quiz_single`, `quiz_multi` | [quiz_single.md](quiz_single.md), [quiz_multi.md](quiz_multi.md) | esercizio |
| `fill_blank` | [fill_blank.md](fill_blank.md) | esercizio |
| `order_steps` | [order_steps.md](order_steps.md) | esercizio |
| `flashcard` | [flashcard.md](flashcard.md) | esercizio |
| `open_answer` | [open_answer.md](open_answer.md) | esercizio |
| `checklist` | [checklist.md](checklist.md) | azione |
| `primary_source`, `lesson_link`, `ask_agent` | [primary_source.md](primary_source.md), [lesson_link.md](lesson_link.md), [ask_agent.md](ask_agent.md) | navigazione |
| `missing_component` + blocco sconosciuto | [missing_component.md](missing_component.md) | fallback |
| Regole comuni degli esercizi | [_exercises.md](_exercises.md) | |

## Regole comuni a tutti i blocchi

**Ritmo verticale** (in colonna di lettura, `readingMaxWidth` 600; i valori sono token `spacing`):

| Tra… | Distanza |
|---|---|
| `paragraph` → `paragraph` | 16 dp (`md`) |
| `heading` (qualsiasi livello) → blocco successivo | 12 dp (`blockGapTight`) |
| qualsiasi altro blocco → blocco successivo | 24 dp (`blockGap`) |
| blocco → `heading` livello 3 | 32 dp (`xl`) |
| ultimo blocco di una sezione → `heading` livello 2 della successiva | 48 dp (`xxl`), più la barra "Continua" (vedi `lesson.md`) |
| domanda → feedback dentro un esercizio | 12 dp (`sm`) |

**Un blocco = un nodo logico**, con `id` come chiave stabile (`key = block.id` in `LazyColumn`/`Column`) e per lo stato salvato (risposte, aperto/chiuso, bozza). Nessun blocco ha animazioni all'ingresso oltre a `sectionReveal` della sezione intera.

**Larghezza.** Tutti i blocchi occupano la colonna intera (≤ 600 dp), tranne il testo che non supera mai 600. Nessun blocco crea colonne parallele.

**Lingua** (`lang`): se presente e diverso da `it`, il testo del blocco è marcato `LocaleList(lang)` (TalkBack legge con la voce giusta) e, per i testi in serif, si imposta `hyphens = None` (la sillabazione italiana non vale). Il blocco non cambia aspetto.

**Selezione/copia.** Paragrafi, callout, codice e feedback sono selezionabili (`SelectionContainer` per blocco, non per tutta la lezione, per non rompere i tocchi sui termini). Le opzioni dei quiz e i campi non lo sono.

**Contenuti inattesi.** Il renderer non assume mai che il Validator abbia girato: testo vuoto → il blocco non si disegna (e non conta come esercizio); testo oltre il limite → si disegna intero senza troncare (il limite protegge il Generator, non l'interfaccia); `id` duplicato → vince il primo e il secondo si ignora; campo obbligatorio mancante → blocco non renderizzabile → riga neutra di fallback ([missing_component.md](missing_component.md)). Un errore di parsing di **un** blocco non invalida la lezione.

> Per Grader, Recorder e Planner: [../generator-guidelines.md](../generator-guidelines.md).

## Linee guida generali per il Generator

Da consegnare a `llm-engineer`. Ogni punto ha la sua base didattica; il Validator ne applica una parte (nomi dei codici tra parentesi), il resto è nel prompt e negli eval.

### 1. Cosa vuol dire "lezione fatta bene"

> **Confermate dal proprietario il 2026-11-15**: al massimo 6 sezioni (richiamo + 4 concetti + chiusura, §1.1), esercizio del richiamo `required: false` (§1.2), formula della durata con le costanti di §1.4.

1. **Una vittoria sola, osservabile.** `objective` è una capacità ("Saper prevedere se una variabile è ancora usabile dopo un assegnamento"), non un argomento ("Ownership"). Ogni sezione e ogni esercizio devono servire *quella* capacità; ciò che non serve si toglie (coerenza di Mayer). L'app mostra `objective` sotto il titolo: **non** ripeterlo in un paragrafo iniziale.
2. **Spiega per intero, poi fai richiamare.** Tra due esercizi (o tra l'inizio della sezione e il primo esercizio) ci sono **al massimo 300 parole** di prosa (`paragraph` + `callout`, titolo compreso; il codice non conta). Oltre 350 parole consecutive senza esercizio è un difetto (proposta di `warning`, vedi report). Nessuna sezione di sola spiegazione dopo un'altra di sola spiegazione (retrieval practice, Bjork; segmentazione). Eccezioni: la sezione 0 (intro) e la chiusura non hanno esercizi. Una spiegazione completa non è una spiegazione lunga: 300 parole sono un tetto, non un obiettivo; una sezione che spiega bene in 180 parole resta di 180.
3. **Una idea per sezione.** Sezione = un `heading` livello 2 + **2-4 `paragraph`** (con il `callout` dell'errore tipico la prosa di spiegazione è di 150-300 parole; il codice non conta) + la catena di §1.1 + 1-2 esercizi. **4 o 5 concetti per lezione, quindi 5-6 sezioni h2 con richiamo (§1.2) e chiusura comprese** (`too_long`: max 6; con il richiamo i concetti sono esattamente 4, senza possono essere 5), per una lezione da **10-15 minuti**. Come si arriva a 4 concetti senza riempitivo: §1.5. Il titolo della sezione dice l'idea in modo concreto ("Quando `a` smette di essere valida"), non è un'etichetta ("Introduzione") e **non contiene la risposta** a un esercizio della stessa o delle sezioni successive (l'indice mostra tutti i titoli).
4. **Pre-training.** Se un concetto ha un termine nuovo, il termine entra nel glossario e compare come `[[key]]` alla prima menzione, *prima* del concetto che lo usa (Mayer: pre-training). Un `glossary_term` come blocco solo per i 1-2 termini senza i quali la sezione non si capisce; gli altri restano inline.
5. **Dal concreto all'astratto.** Esempio (codice o situazione) prima della regola generale, poi la regola, poi un caso nuovo da risolvere. Il caso da risolvere nell'esercizio deve essere **diverso** dall'esempio spiegato (transfer), non lo stesso con altri nomi.
6. **L'esercizio verifica la capacità dell'`objective`.** Se l'obiettivo è "prevedere", l'esercizio chiede una previsione, non di riconoscere una definizione. Ordine di preferenza (richiamo > riconoscimento): `open_answer` e `fill_blank` > `order_steps` > `quiz_*` > `flashcard` (la flashcard è per fatti da fissare, non per capire).
7. **Mix di esercizi.** Mai tre esercizi dello stesso tipo di fila né più di 2 dello stesso tipo nella lezione tranne `quiz_single`. Il primo esercizio della lezione è il più facile (vittoria presto); l'ultimo richiede di applicare quanto appreso in un caso nuovo. Una lezione deve avere almeno un esercizio `required` (`no_required_exercise`), tipicamente 1 per sezione, ma non obbligatoriamente in ogni sezione; le sezioni 0 e finale possono non averne.
8. **Chiusura.** Ultima sezione: sintesi in 2-4 righe di `paragraph` (non un elenco ripetuto di tutto), eventualmente `checklist` di azioni concrete nel mondo reale, **un** `primary_source` ("Per approfondire", `primary_source_count`), eventuali `lesson_link`, **almeno un** `ask_agent` (`ask_agent_missing`).

### 1.1 La catena di un concetto: intuizione, esempio svolto, regola, errore tipico

Ogni sezione di concetto (cioè ogni sezione che non è intro, richiamo o chiusura) spiega **un solo concetto** con questa sequenza, **in questo ordine**, prima degli esercizi. Dal piano: `example` e `steps` alimentano l'esempio svolto, `pitfall` l'errore tipico. Se il piano non dà un `pitfall` per il concetto, l'anello 4 si omette (non si inventa); gli altri tre non si omettono mai.

| # | Anello | Blocchi | Misura | Cosa contiene |
|---|---|---|---|---|
| 1 | **Intuizione** | 1 `paragraph` | 40-80 parole | Una situazione concreta o una domanda che il concetto risolve, in parole di tutti i giorni. I termini nuovi compaiono qui come `[[key]]` alla prima menzione (pre-training). Nessuna definizione formale, nessun `**grassetto**` di regola |
| 2 | **Esempio svolto** | `heading` livello 3 «Esempio: <caso concreto>» + (opzionale) `code` + 1 `paragraph` | codice 3-12 righe; paragrafo 50-100 parole | Un caso con valori concreti risolto **passo per passo** ("Prima…, poi…, quindi…"), a parole in prosa, mai in elenco. Per argomenti di codice il `code` precede il paragrafo; per gli altri argomenti il caso è nel paragrafo, con numeri o nomi reali. Mai il caso dell'esercizio (transfer: l'esercizio ne usa uno nuovo). Dalla fonte, se esiste: stesso esempio parafrasato con `[^key]` |
| 3 | **Regola** | 1 `paragraph` | 30-70 parole | La generalizzazione in una o due frasi con la formulazione chiave in **grassetto** (1 sola volta), dopo l'esempio: dal concreto all'astratto. Citazione `[^key]` se la fonte la sostiene |
| 4 | **Errore tipico** | 1 `callout` `variant: warning` | titolo ≤ 8 parole che comincia con «Errore tipico:»; testo ≤ 300 caratteri | Tre mosse: cosa si tende a credere, perché sembra ragionevole, come stanno le cose. Niente colpa, niente «attenzione a non sbagliare»: è la convinzione che un distrattore dei quiz della stessa sezione mette alla prova (il feedback di quel distrattore la nomina) |
| 5 | Esercizi | 1-2 esercizi | vedi le regole per tipo più sotto | Applicano la regola a un caso nuovo in cui l'errore tipico è la strada più facile. Mai prima dell'anello 3 |

Conseguenze verificabili:
- Gli anelli 1-4 occupano **al massimo 300 parole** (regola 2). Se non ci stanno, il concetto è troppo grosso: si divide in due sezioni, non si accorcia l'esempio.
- I paragrafi della sezione sono 3 (intuizione, passo per passo, regola), al massimo 4: il quarto è una seconda tappa dell'esempio o un caso limite, e va separato dal precedente da un `code` o dal `callout`.
- **Niente muro di testo** sul tablet orizzontale (colonna 600 dp, ≈ 63 caratteri per riga, ≈ 19 righe per 180 parole): non più di **2 `paragraph` consecutivi** (≈ 180 parole, circa una schermata a 1280 × 800); fra il 2° e un eventuale 3° si interpone un blocco di altro tipo (`heading` 3, `code`, `callout`, `glossary_term`, `order_steps`...). La sequenza della tabella lo rispetta: intuizione, h3, [codice], passo per passo, regola, callout.
- Un `paragraph` ha **40-100 parole** (≈ 300-650 caratteri); più lungo si divide. Frasi ≤ 25 parole (§2).
- Ogni sezione di concetto ha **esattamente un** `heading` livello 3 (l'esempio), e nessun altro. Le sezioni 0, richiamo e chiusura non hanno h3.

### 1.2 Richiamo iniziale dei prerequisiti

Il piano divide i prerequisiti per base: `record` (dimostrato in lezioni passate), `mission` (dichiarato dalla persona), `general` («da richiamare»: nessuna prova che la persona lo sappia).

- **Quando c'è.** Se il piano ha **almeno un** prerequisito `general`. **Si omette** del tutto se ha zero prerequisiti `general` (anche se ne ha di `record` e `mission`) o se la lezione non ha prerequisiti: nessuna sezione vuota, nessuna frase «ripassiamo un po'».
- **Dove.** È la **prima sezione h2** (subito dopo l'intro, prima di ogni concetto) e conta nelle 5-6 sezioni. Titolo concreto: «Cosa ti serve: <A> e <B>» (≤ 8 parole di sostanza).
- **Quanti.** Al massimo **3** prerequisiti. Se i `general` sono di più, si richiamano i 3 da cui il primo concetto dipende di più; gli altri si introducono inline alla prima menzione (con `[[key]]`), come ogni termine nuovo.
- **Come.** Composizione fissa, **≤ 300 parole in tutto**: (a) 1 `paragraph` di 20-40 parole: «Questa lezione usa A e B. Prova a rispondere: se qualcosa non torna, il ripasso è subito sotto.»; (b) **1 esercizio** `quiz_single` o `fill_blank` (retrieval prima della rilettura, Bjork) con `required: false`, che chiede di **usare** il prerequisito più importante in un caso piccolo (non di ricordare una definizione); (c) per ciascun prerequisito 1 `paragraph` di 25-60 parole «ripasso»: l'idea in 2-3 frasi con un mini-esempio. Il feedback dell'esercizio non ripete il paragrafo di ripasso: rimanda a quello.
- **Perché `required: false`.** Chi sa già il prerequisito non deve essere bloccato (nessuna penalità, P9); chi non lo sa lo trova subito sotto. La sezione non ha esercizi obbligatori e «Continua» è subito attivo; il `required` della lezione sta nelle sezioni di concetto.
- **Niente flashcard** nel richiamo (non sono ripasso della lezione e non devono produrre schede di ripasso di ciò che era un prerequisito).
- **Per `record`**: non si ripassa; al più mezza frase nel testo che lo usa. **Per `mission`**: non c'è una sezione, ma alla **prima menzione** nel testo del concetto che lo usa si aggiunge una clausola di richiamo (≤ 15 parole: «ricorda: …»), perché dichiarato non vuol dire dimostrato.

### 1.3 Non dare nulla per scontato

- Ogni termine tecnico che non è tra i prerequisiti `record`/`mission` e non è nel `title` della lezione ha una definizione **alla prima occorrenza**: `[[key]]` (glossario) o una definizione nella stessa frase. Verifica: scorrendo il testo, nessun termine tecnico appare prima del suo `[[key]]`.
- Mai le formule che presuppongono: «ovviamente», «naturalmente», «banalmente», «come è noto», «semplicemente», «basta» (davanti a un'operazione), «è facile vedere», «chiaramente».
- Un passaggio dell'esempio svolto non si salta: se il paragrafo dice «quindi», il passo che porta al risultato è scritto.

### 1.4 Intro, chiusura e budget di parole

| Parte | Contenuto | Parole di prosa |
|---|---|---|
| Sezione 0 (intro) | 1-2 `paragraph`: dove si colloca nell'argomento e che cosa si saprà fare alla fine (senza ripetere `objective`); per i PDF l'ancora di `generator-guidelines.md` §6 | ≤ 100 |
| Richiamo (§1.2) | solo se ci sono prerequisiti `general` | ≤ 300 |
| Ogni sezione di concetto | catena §1.1 | spiegazione 150-300 + esercizi 100-200 |
| Chiusura | sintesi 2-4 righe, `checklist`, `primary_source`, `ask_agent` | ≤ 120 |
| **Lezione** | tutto (la definizione di §9 del contratto: `paragraph`, `callout`, domande, opzioni, feedback) | **obiettivo 1500-1900; minimo 1200; limite 2200** (`too_long`) |

Ripartizione indicativa dentro i 2200: spiegazione (paragrafi e callout) ≤ 1500, prosa degli esercizi (domande, opzioni, feedback) ≤ 700. Se la lezione sfora, si toglie un concetto o un esempio secondario, **non** il feedback degli esercizi né l'errore tipico.

**Durata.** `estimated_minutes = round(P / 250 + 0,5 × E + 2 × O)` con `P` parole di prosa totali, `E` numero di esercizi (almeno 1 per concetto: 5-9 con 4-5 concetti) e `O` numero di `open_answer` (già contati in `E`). Con `P` = 1500-1900, `E` = 5-9 e `O` = 1 dà 10-15; fuori da 10-15 la lezione è troppo corta o troppo lunga. **La formula la applica il backend sulle parole reali del documento**: il Generator non calcola né deve indovinare la durata (se il campo c'è, il backend lo sovrascrive). Con 4-5 concetti da ~330-380 parole ciascuno (spiegazione ~300 + esercizi) il totale cade nell'obiettivo; con 2-3 concetti no, ed è il motivo del minimo di 4.

### 1.5 Come si arriva a 4 concetti senza riempitivo

Ogni lezione ha **4 o 5 concetti** (decisione del 2026-11-11, misurata sugli eval: il modello scrive ~330-380 parole per concetto, quindi 1500-1900 parole e 10-15 minuti richiedono 4-5 concetti). Il rischio è il concetto aggiunto solo per fare volume: va presidiato con un criterio, non con la quantità.

**Test di ammissione (ogni concetto, anche i primi 2-3).** Un concetto esiste solo se vale **tutte** queste condizioni:
1. **Capacità distinta.** Ha un esercizio che richiede un ragionamento che gli esercizi degli altri concetti non richiedono: se si può rispondere con ciò che si è imparato in un altro concetto, non è un concetto.
2. **Serve all'`objective`.** Togliendolo, la persona non potrebbe più *dimostrare* l'`objective` (o lo farebbe con un buco). Concetto non necessario = coerenza violata (Mayer).
3. **Non è l'anticipazione di un'altra lezione.** Non serve a ciò che si farà dopo, ma a ciò che si dimostra in questa.
4. **Ha un `example` proprio** (valori e passi diversi dagli altri concetti).

**Quando l'argomento ha meno di 4 idee proprie, si può aggiungere (in quest'ordine di preferenza):**
- **Prerequisito promosso.** Un prerequisito `general` troppo grande per un richiamo di ≤ 300 parole (non si spiega in 2-3 frasi con un mini-esempio) diventa concetto di supporto, **con la catena completa di §1.1 e i suoi esercizi**, e allora la sezione di richiamo *non* c'è per quel prerequisito. Va prima dei concetti che lo usano.
- **Applicazione o caso limite con ragionamento nuovo.** Un concetto che mette il principio sotto vincolo (un caso in cui la regola base non basta, l'interazione con un secondo meccanismo, un errore che emerge solo nel caso composto), con un esercizio che richiede *prevedere* in una situazione non ancora vista. Non l'applicazione più grande dello stesso esempio.
- **Confronto con ciò che la persona sa già.** Il punto di partenza (record `record` e dichiarazioni della `mission`) ha un modello vicino ma diverso (un altro linguaggio, uno strumento simile): un concetto che dice **dove il modello noto smette di valere** e perché. Solo se esiste davvero quel punto di partenza; mai un confronto generico con «altri linguaggi».

**Non si aggiunge mai:**
- lo stesso concetto con un altro esempio, un'altra parola o un'altra formulazione (è ripetizione, non nuova capacità; il transfer si fa negli esercizi);
- un concetto fuori dalla `mission` o che non serve all'`objective` (digressione, storia, curiosità);
- un'anticipazione della lezione successiva («nella prossima lezione vedremo…» promosso a sezione);
- un concetto la cui sezione è un elenco o un riassunto dei precedenti.

**Come lo verifica un giudice.** Per ogni coppia di concetti (A, B): (a) l'esercizio di B si risolve usando solo A? se sì, B è ripetizione; (b) il piano dichiara per ciascun concetto il **motivo di ammissione** (`nuova capacità` | `prerequisito promosso` | `caso limite` | `confronto col già noto`) e un'`objective` ridotta a una frase che lo collega all'`objective` della lezione; (c) nessun concetto ha come unico `example` una variante di quello di un altro. Se non si trovano 4 concetti che passano il test, il difetto è **nell'argomento scelto dal Planner** (troppo stretto): lo si segnala nel ragionamento del piano e lo si allarga all'`objective` vicino, non lo si riempie.

### 2. Scrivere i testi (valido per tutti i blocchi)

- **Italiano chiaro**, frasi di ≤ 25 parole, voce attiva, seconda persona singolare quando ci si rivolge all'utente ("hai", "prova"). Termini tecnici nella forma originale della comunità (ADR 0005 D3), spiegati nel glossario alla prima occorrenza. Niente gergo gratuito, niente "come abbiamo visto" (la sezione si rilegge a pezzi).
- **Tono**: sobrio, mai celebrativo né colpevolizzante. Vietati nei testi: "Bravo!", "Ottimo lavoro!", "Sbagliato!", "Facile", "Semplicemente", "ovviamente", "banale" (sminuiscono chi non ha capito), punti esclamativi su errori.
- **Segnalazione, con parsimonia**: `**grassetto**` solo per il termine o la regola centrale della frase, al massimo 1-2 volte per paragrafo, mai intere frasi. Il corsivo per termini menzionati o enfasi leggera. Nessun MAIUSCOLO.
- **Codice inline** per identificatori, comandi, valori letterali (`let b = a;`); mai per enfasi.
- **Citazioni `[^key]`**: dopo l'affermazione che la fonte sostiene, a fine frase, non a metà. Ogni fonte in `sources` va citata almeno una volta o usata da `primary_source` (`unused_entry`). Niente più di 2 citazioni per paragrafo.
- **Link `[testo](https://…)`**: quasi mai; per le fonti c'è `sources`. Il testo del link dice dove porta ("la guida ufficiale su X"), mai "clicca qui".
- **Citazioni in lingua originale**: in un `paragraph` separato con `lang`, seguite da una parafrasi in italiano nel paragrafo successivo (ADR 0005 D3).
- **Nessuna struttura dentro un campo *md***: niente elenchi nei paragrafi. Se servono passi sequenziali in elenco, sono un `order_steps` o più `paragraph`. Se servono 3+ elementi paralleli: un `callout` o paragrafi brevi.
- **Lunghezze consigliate** (limiti del contratto tra parentesi): paragrafo 300-650 caratteri, 40-100 parole (1200); callout ≤ 300 (600); domanda di quiz ≤ 200 (400); opzione ≤ 100 (200); feedback ≤ 250 (400); `front` della flashcard ≤ 150 (300), `back` ≤ 300 (600).

### 3. Callout

Max 1 per sezione. `warning` = **errore tipico** del concetto, uno per sezione di concetto (§1.1 anello 4: titolo «Errore tipico: …»), quindi fino a 5 per lezione; `tip` e `info` insieme al massimo 2 per lezione. `tip` = tecnica per ricordare/applicare; `info` = contesto laterale che non serve all'obiettivo (se serve, va nel paragrafo). Mai un callout come unico contenuto di una sezione. Mai per la definizione chiave (usa glossario/paragrafo con grassetto). Mai per esercizi o istruzioni di esercizio.

### 4. Codice

Esempi **minimi**: 3-12 righe, ≤ 60 caratteri per riga (nessuno scorrimento orizzontale su tablet; il limite del contratto è 100). Un solo concetto per esempio; niente codice di contorno non necessario (`use`, `main` boilerplate) se non è l'oggetto. Un commento in codice solo se necessario a leggere l'esempio. Se il paragrafo rimanda a una riga, dica "alla riga 2" (l'app mostra i numeri di riga dal 3° rigo in su, vedi [code.md](code.md)). Il `language` va indicato (`text` per output/diagrammi ASCII). Esempio **prima** del quiz che lo usa, mai dopo.

### 5. Quiz (`quiz_single`, `quiz_multi`)

- **Un solo concetto per domanda**, formulata in modo che la risposta si possa dare *prima* di guardare le opzioni (domanda completa, non "Quale di queste è vera?").
- **Distrattori plausibili**: ognuno rappresenta un errore o una misconcezione reale (di cui `feedback` dice quale), non un'assurdità né una trappola linguistica. Mai "Nessuna delle precedenti" / "Tutte le precedenti".
- **Opzioni omogenee**: stessa struttura grammaticale, stessa lunghezza (`quiz_option_length`, max scarto 3 parole), stessa precisione; la corretta non è la più lunga né la più specifica; niente assoluti ("sempre", "mai") solo nei distrattori; niente grassetto/termini/citazioni nelle opzioni.
- **Posizione della corretta variabile** nella lezione (`quiz_answer_position`); non ordinare le opzioni per correttezza.
- **`feedback` di ogni opzione, scritto per chi l'ha scelta**: nella corretta, perché è giusta (e la regola sottostante); nelle sbagliate, *perché sembrava ragionevole* e perché non regge ("Sembra giusto perché… ma…"). Mai "Sbagliato." da solo, mai ripetere l'opzione. Il feedback è letto quando l'utente ha scelto *quell'opzione* (e per la corretta, anche quando non l'ha scelta): deve reggersi da solo, senza riferimenti a "la B" o a "sopra" (l'ordine è lo stesso, ma sia robusto).
- `quiz_multi`: chiarire nella domanda che ce ne sono più di una **senza dirne il numero**; 3-6 opzioni, 1..(n-1) corrette; usarlo per "quali tra" (classificare), non per elencare tutto.

### 6. Fill-in-the-blank

Una sola frase (o poche) con 1-3 vuoti; ogni vuoto è **una parola o un token** (identificatore, parola chiave, valore), mai una frase. Il contesto attorno deve determinare la risposta univocamente. `answers`: includere sinonimi e varianti accettabili (`move`, `moved`; `&mut`), forme con e senza articolo se plausibile, 1-5 voci. `case_sensitive: true` solo per linguaggi/identificatori sensibili alle maiuscole. Il testo non deve rivelare la risposta (es. "la ... (move)"). Non mettere il vuoto come primo né ultimo carattere del testo.

### 7. Ordinamento (`order_steps`)

3-8 passi **realmente sequenziali** (un procedimento, una derivazione), ognuno ≤ 100 caratteri, iniziati dalla stessa forma verbale (infinito o imperativo), senza numerazione e senza marcatori di sequenza ("prima", "infine", "poi", "al passo 2"), perché sarebbero indizi. I passi sono forniti nell'ordine corretto; l'app li mescola. Non usarlo per elenchi senza ordine intrinseco (ordinamento arbitrario = nessun apprendimento).

### 8. Flashcard

Solo per **fatti da fissare** (definizioni, firme, differenze), una per sezione al massimo, `front` = una domanda precisa a risposta breve, `back` = la risposta in 1-3 frasi + (opzionale) la ragione. Mai risposta già contenuta nel `front`. Diventeranno schede di ripasso : devono avere senso fuori dal contesto della lezione (niente "come nell'esempio sopra").

### 9. Risposta aperta

Usarla 1 volta per lezione al massimo, tipicamente nella sezione finale, per **spiegare con parole proprie** o **prevedere e giustificare**. La `question` chiede una produzione specifica ("Spiega in 2-3 frasi perché `a` non è più valida dopo `let b = a;`"); `max_chars` 300-600 di norma. `rubric` (solo backend): criteri osservabili (3-5 punti chiave attesi, errori tipici, cosa conta come risposta parziale). Imposta `required: false` se è una seconda sfida di approfondimento, non l'esercizio unico della lezione.

### 10. Checklist

Solo azioni da fare **fuori** dall'app (provare un comando, leggere una sezione del libro, scrivere un esempio), formulate al verbo ("Scrivi un esempio che…"), 2-5 voci. Mai per ripetere i concetti. Mai l'unico esercizio.

### 11. `missing_component`

Se per un bisogno didattico reale non c'è un blocco adatto, il Generator **non forza** un blocco inadatto (es. un quiz per mostrare un diagramma): usa `missing_component`.

- `wanted`: nome `snake_case` del componente ideale (`comparison_table`, `timeline`, `step_through_code`); `purpose`: la **funzione didattica** (non l'aspetto): "mostrare come il valore passa da `a` a `b` in 3 istanti".
- `fallback`: 1-6 blocchi che ottengono il meglio possibile con il catalogo (es. tabella di confronto → paragrafi brevi; i diagrammi di flusso, gerarchia e ciclo hanno già il blocco `diagram`). **La lezione deve funzionare per intero usando solo il fallback**: niente rimandi al componente mancante ("vedi la figura").
- Al massimo 2 per lezione. Mai per aggirare un limite (testo troppo lungo, più di 6 sezioni). Mai l'unico esercizio né l'unico contenuto di una sezione. Mai annidati.
- L'app non mostra `wanted` né `purpose` all'utente: sono per il team (lista di desideri dei componenti).

### 12. Controlli di qualità da fare prima di consegnare

1. Letto in ordine, ogni sezione si capisce senza le successive? 2. La domanda di ogni esercizio è risolvibile con quanto spiegato *prima* di essa? 3. Nessun esercizio ha la risposta nel testo del titolo, dell'opzione o del feedback di un altro esercizio *precedente*? 4. Ogni distrattore ha un feedback che nomina la misconcezione? 5. Il totale di prosa è 1500-1900 parole (minimo 1200, limite 2200) e la durata calcolata dal backend (§1.4) è 10-15? Ci sono 4-5 concetti, tutti superano il test di §1.5? 6. Termini nuovi: tutti nel glossario e usati come `[[key]]` prima del concetto? 7. L'`objective` è dimostrato da almeno un esercizio `required`? 8. Ogni sezione di concetto ha la catena di §1.1 nell'ordine (intuizione, h3 esempio, regola, callout errore tipico) e al massimo 300 parole prima dell'esercizio? 9. Mai più di 2 paragrafi consecutivi? 10. Il richiamo c'è se e solo se il piano ha prerequisiti `general`?

## Costanti di lezione (per android-dev)

`ExerciseCard` padding 20 dp (16 in compact), raggio `md`, fill `surface`, bordo `line` 1 dp. `choiceMinHeight` 56. `blankFieldWidth` 120, `blankFieldHeight` 40. `codeCollapsedLines` 16. `contextPaneBreakpoint` 1100, `contextPaneGap` 32. Tutti in `tokens.json` (`layout`).
