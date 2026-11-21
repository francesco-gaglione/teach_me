# ADR 0006 — FSRS: crate rs-fsrs dietro ReviewScheduler

- Stato: sostituita da ADR 0014 (FSRS-7, 2026-11-14)
- Data: 2026-11-09
- Attua la decisione D4 di ADR 0005, dopo la verifica della libreria. Contratto: `docs/contracts/reviews.md`.

## Scelta

**`rs-fsrs` 1.2.1**, licenza MIT (`license-file`, quindi crates.io la riporta come "non-standard": un eventuale `cargo deny` va configurato di conseguenza), dietro il port `ReviewScheduler` del dominio. Adapter: `backend/crates/infrastructure/src/fsrs.rs` (`FsrsScheduler`). Parametri: pesi di default del crate (FSRS-5, 19 pesi), retention 0,9, `enable_short_term = true`, `enable_fuzz = false`. Nessun tipo di `rs_fsrs` o `chrono` esce da `fsrs.rs`.

Motivi:
- È l'unico dei due che è uno scheduler completo: stati New/Learning/Review/Relearning e passi brevi (di nuovo 1 min, difficile 5 min, bene 10 min; in apprendimento/riapprendimento di nuovo 5 min, difficile 10 min), come chiede `reviews.md` §1.
- Una sola dipendenza (`chrono`, che resta confinata all'adapter), circa 1.400 righe: leggibile e controllabile per intero.
- Fuzz disattivato per default e un solo punto che lo legge (`apply_fuzz`); l'esito è deterministico, quindi l'anteprima degli intervalli coincide con quanto applicato (testato).
- Corretto contro i casi di riferimento: la sequenza di intervalli e di stati dei test del crate (derivati da py-fsrs/ts-fsrs, 13 voti con pesi di default) è riprodotta dal nostro adapter (`follows_the_reference_interval_sequence`: `[0,4,15,48,136,351,0,0,7,13,24,43,77]` giorni), più le proprietà note a retention 0,9 (intervallo = stabilita`, quindi "facilissima" su scheda nuova = 15 giorni = w[3]; stabilità dopo "bene" su scheda nuova = w[2] = 3,1262).

## Alternative scartate

- **`fsrs` (fsrs-rs) 6.6.2**, BSD-3-Clause, ultima release 2026-08-29, mantenuto attivamente, FSRS-6 (21 pesi), ottimizzatore e calcolo della memoria. Scartato per l'MVP: non gestisce stati e passi di apprendimento (restituisce solo lo stato di memoria e un intervallo in giorni frazionari per ciascun voto), quindi "di nuovo tra pochi minuti" e il ciclo Learning/Relearning andrebbero scritti da noi, cioè una parte dello scheduler sarebbe nostra comunque. Non l'ho compilato: dal manifesto su crates.io le dipendenze normali sono `ndarray`, `rayon`, `rand`, `snafu`, `strum`, `itertools`, `priority-queue`, `serde`, `log`; `burn` risulta solo dev-dependency (da riverificare alla prima adozione). Resta la strada naturale se in futuro serviranno FSRS-6 o l'ottimizzazione dei pesi sui log personali (fuori da `reviews.md` v1).
- **Implementazione nostra nel dominio**: non necessaria, dato che il crate passa i casi di riferimento. Il port la rende comunque possibile senza toccare `application`.

## Rischi

- **Manutenzione**: l'ultima release (1.2.1) è del 2024-10-28, circa 13 mesi fa, versione unica pubblicata, nessun aggiornamento a FSRS-6. Mitigazione: crate piccolo, codice letto per intero, test di riferimento nel nostro repo; se muore si porta l'algoritmo nel dominio o si passa a `fsrs` mantenendo i test.
- **FSRS-5, non FSRS-6**: i pesi di default sono quelli di FSRS-5; la qualità delle previsioni è leggermente inferiore a FSRS-6. Accettabile per un utente singolo senza dati di ottimizzazione.
- **Discrepanze minori con py-fsrs**: i passi sono fissi nel crate (1/5/10 min), non configurabili; py-fsrs usa 1 min e 10 min come passi e interpola il "difficile". Non influisce sulla correttezza a lungo termine.
- **Granularità in giorni interi**: `elapsed_days` è il numero di giorni *interi* dall'ultimo voto (arrotondato per difetto) e gli intervalli lunghi sono multipli di 24 h esatte dall'istante del voto (non "a mezzanotte"). Coerente con l'anteprima.
- **Licenza non-standard** nel metadato (testo MIT in `LICENSE`).
- **Dipendenza transitiva**: `chrono` porta `iana-time-zone` e `num-traits` perchè `rs-fsrs` usa `Utc::now()` per i valori di default; nessun impatto a runtime perchè passiamo sempre l'istante esplicito.
